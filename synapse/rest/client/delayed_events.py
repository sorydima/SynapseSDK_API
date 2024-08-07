#
# This file is licensed under the Affero General Public License (AGPL) version 3.
#
# Copyright (C) 2024 New Vector, Ltd
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as
# published by the Free Software Foundation, either version 3 of the
# License, or (at your option) any later version.
#
# See the GNU Affero General Public License for more details:
# <https://www.gnu.org/licenses/agpl-3.0.html>.
#
# Originally licensed under the Apache License, Version 2.0:
# <http://www.apache.org/licenses/LICENSE-2.0>.
#
#

""" This module contains REST servlets to do with delayed events: /delayed_events/<paths> """
import logging
from http import HTTPStatus
from typing import TYPE_CHECKING, List, Tuple

from synapse.api.errors import Codes, SynapseError
from synapse.http.server import HttpServer
from synapse.http.servlet import RestServlet, parse_json_object_from_request
from synapse.http.site import SynapseRequest
from synapse.rest.client._base import client_patterns
from synapse.types import JsonDict

if TYPE_CHECKING:
    from synapse.server import HomeServer

logger = logging.getLogger(__name__)


# TODO: Needs unit testing
class UpdateDelayedEventServlet(RestServlet):
    PATTERNS = client_patterns(
        r"/org\.matrix\.msc4140/delayed_events/(?P<delay_id>[^/]*)$",
        releases=(),
    )
    CATEGORY = "Delayed event management requests"

    def __init__(self, hs: "HomeServer"):
        super().__init__()
        self.auth = hs.get_auth()
        self.delayed_events_handler = hs.get_delayed_events_handler()

    async def on_POST(
        self, request: SynapseRequest, delay_id: str
    ) -> Tuple[int, JsonDict]:
        requester = await self.auth.get_user_by_req(request)

        body = parse_json_object_from_request(request)
        try:
            action = str(body["action"])
        except KeyError:
            raise SynapseError(
                HTTPStatus.BAD_REQUEST,
                "'action' is missing",
                Codes.MISSING_PARAM,
            )

        await self.delayed_events_handler.update(requester, delay_id, action)
        return 200, {}


# TODO: Needs unit testing
class DelayedEventsServlet(RestServlet):
    PATTERNS = client_patterns(
        r"/org\.matrix\.msc4140/delayed_events$",
        releases=(),
    )
    CATEGORY = "Delayed event management requests"

    def __init__(self, hs: "HomeServer"):
        super().__init__()
        self.auth = hs.get_auth()
        self.delayed_events_handler = hs.get_delayed_events_handler()

    async def on_GET(self, request: SynapseRequest) -> Tuple[int, List[JsonDict]]:
        requester = await self.auth.get_user_by_req(request)
        # TODO: Support Pagination stream API ("from" query parameter)
        data = await self.delayed_events_handler.get_all_for_user(requester)
        return 200, data


def register_servlets(hs: "HomeServer", http_server: HttpServer) -> None:
    UpdateDelayedEventServlet(hs).register(http_server)
    DelayedEventsServlet(hs).register(http_server)
