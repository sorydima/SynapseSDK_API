"""Tests REST events for /delayed_events paths."""

from http import HTTPStatus
from typing import List

from twisted.test.proto_helpers import MemoryReactor

from synapse.rest.client import delayed_events, room
from synapse.server import HomeServer
from synapse.types import JsonDict
from synapse.util import Clock

from tests.unittest import HomeserverTestCase

_HS_NAME = "red"
_EVENT_TYPE = "com.example.test"


class DelayedEventsTestCase(HomeserverTestCase):
    """Tests getting and managing delayed events."""

    servlets = [delayed_events.register_servlets, room.register_servlets]
    user_id = f"@sid1:{_HS_NAME}"

    def default_config(self) -> JsonDict:
        config = super().default_config()
        config["server_name"] = _HS_NAME
        config["max_event_delay_duration"] = "24h"
        return config

    def prepare(self, reactor: MemoryReactor, clock: Clock, hs: HomeServer) -> None:
        self.room_id = self.helper.create_room_as(
            self.user_id,
            extra_content={
                "preset": "trusted_private_chat",
            },
        )

    def test_delayed_events_empty_on_startup(self) -> None:
        self.assertListEqual([], self._get_delayed_events())

    def test_delayed_state_events_are_sent_on_timeout(self) -> None:
        state_key = "to_send_on_timeout"

        setter_key = "setter"
        setter_expected = "on_timeout"
        channel = self.make_request(
            "PUT",
            _get_path_for_delayed_state(self.room_id, _EVENT_TYPE, state_key, 900),
            {
                setter_key: setter_expected,
            },
        )
        self.assertEqual(HTTPStatus.OK, channel.code, channel.result)
        events = self._get_delayed_events()
        self.assertEqual(1, len(events), events)
        content = self._get_delayed_event_content(events[0])
        self.assertEqual(setter_expected, content.get(setter_key), content)
        self.helper.get_state(
            self.room_id,
            _EVENT_TYPE,
            "",
            state_key=state_key,
            expect_code=HTTPStatus.NOT_FOUND,
        )

        self.reactor.advance(1)
        self.assertListEqual([], self._get_delayed_events())
        content = self.helper.get_state(
            self.room_id,
            _EVENT_TYPE,
            "",
            state_key=state_key,
        )
        self.assertEqual(setter_expected, content.get(setter_key), content)

    def test_delayed_state_events_are_cancelled_by_more_recent_state(self) -> None:
        state_key = "to_be_cancelled"

        setter_key = "setter"
        channel = self.make_request(
            "PUT",
            _get_path_for_delayed_state(self.room_id, _EVENT_TYPE, state_key, 900),
            {
                setter_key: "on_timeout",
            },
        )
        self.assertEqual(HTTPStatus.OK, channel.code, channel.result)
        events = self._get_delayed_events()
        self.assertEqual(1, len(events), events)

        setter_expected = "manual"
        self.helper.send_state(
            self.room_id,
            _EVENT_TYPE,
            {
                setter_key: setter_expected,
            },
            None,
            state_key=state_key,
        )
        self.assertListEqual([], self._get_delayed_events())

        self.reactor.advance(1)
        content = self.helper.get_state(
            self.room_id,
            _EVENT_TYPE,
            "",
            state_key=state_key,
        )
        self.assertEqual(setter_expected, content.get(setter_key), content)

    def _get_delayed_events(self) -> List[JsonDict]:
        channel = self.make_request(
            "GET", b"/_matrix/client/unstable/org.matrix.msc4140/delayed_events"
        )
        self.assertEqual(HTTPStatus.OK, channel.code, channel.result)

        key = "delayed_events"
        self.assertIn(key, channel.json_body)

        events = channel.json_body[key]
        self.assertIsInstance(events, list)

        return events

    def _get_delayed_event_content(self, event: JsonDict) -> JsonDict:
        key = "content"
        self.assertIn(key, event)

        content = event[key]
        self.assertIsInstance(content, dict)

        return content


def _get_path_for_delayed_state(
    room_id: str, event_type: str, state_key: str, delay_ms: int
) -> str:
    return f"rooms/{room_id}/state/{event_type}/{state_key}?org.matrix.msc4140.delay={delay_ms}"
