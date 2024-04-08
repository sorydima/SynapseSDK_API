/*
 * This file is licensed under the Affero General Public License (AGPL) version 3.
 *
 * Copyright (C) 2024 New Vector, Ltd
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * See the GNU Affero General Public License for more details:
 * <https://www.gnu.org/licenses/agpl-3.0.html>.
 */

use std::{collections::HashMap, time::Duration};

use bytes::Bytes;
use headers::{
    AccessControlAllowOrigin, AccessControlExposeHeaders, ContentLength, ContentType, HeaderMapExt,
    IfMatch, IfNoneMatch, Pragma,
};
use http::{header::ETAG, HeaderMap, Response, StatusCode, Uri};
use mime::Mime;
use pyo3::{
    exceptions::PyValueError, pyclass, pymethods, types::PyModule, PyAny, PyResult, Python,
};
use ulid::Ulid;

use self::session::Session;
use crate::{
    errors::{HeaderMapPyExt, NotFoundError, PayloadTooLargeError, SynapseError},
    http::{http_request_from_twisted, http_response_to_twisted},
};

mod session;

const MAX_CONTENT_LENGTH: u64 = 1024 * 100;

fn prepare_headers(headers: &mut HeaderMap, session: &Session) {
    headers.typed_insert(AccessControlAllowOrigin::ANY);
    headers.typed_insert(AccessControlExposeHeaders::from_iter([ETAG]));
    headers.typed_insert(Pragma::no_cache());
    headers.typed_insert(session.etag());
    headers.typed_insert(session.expires());
    headers.typed_insert(session.last_modified());
}

fn check_input_headers(headers: &HeaderMap) -> PyResult<Mime> {
    let ContentLength(content_length) = headers.typed_get_required()?;

    if content_length > MAX_CONTENT_LENGTH {
        return Err(PayloadTooLargeError::new());
    }

    let content_type: ContentType = headers.typed_get_required()?;

    Ok(content_type.into())
}

// TODO: handle eviction
#[pyclass]
struct RendezVousHandler {
    base: Uri,
    sessions: HashMap<Ulid, Session>,
}

#[pymethods]
impl RendezVousHandler {
    #[new]
    fn new(homeserver: &PyAny) -> PyResult<Self> {
        let base: String = homeserver
            .getattr("config")?
            .getattr("server")?
            .getattr("public_baseurl")?
            .extract()?;
        let base = Uri::try_from(format!(
            "{base}_matrix/client/unstable/org.matrix.msc4108/rendezvous"
        ))
        .map_err(|_| PyValueError::new_err("Invalid base URI"))?;

        Ok(Self {
            base,
            sessions: HashMap::new(),
        })
    }

    fn handle_post(&mut self, twisted_request: &PyAny) -> PyResult<()> {
        let request = http_request_from_twisted(twisted_request)?;

        let content_type = check_input_headers(request.headers())?;

        let id = Ulid::new();

        let uri = format!("{base}/{id}", base = self.base);

        let body = request.into_body();

        let session = Session::new(body, content_type, Duration::from_secs(5 * 60));

        let response = serde_json::json!({
            "url": uri,
        })
        .to_string();

        let mut response = Response::new(response.as_bytes());
        *response.status_mut() = StatusCode::CREATED;
        response.headers_mut().typed_insert(ContentType::json());
        prepare_headers(response.headers_mut(), &session);
        http_response_to_twisted(twisted_request, response)?;

        self.sessions.insert(id, session);

        Ok(())
    }

    fn handle_get(&mut self, twisted_request: &PyAny, id: &str) -> PyResult<()> {
        let request = http_request_from_twisted(twisted_request)?;

        let if_none_match: Option<IfNoneMatch> = request.headers().typed_get_optional()?;

        let id: Ulid = id.parse().map_err(|_| NotFoundError::new())?;
        let session = self.sessions.get(&id).ok_or_else(NotFoundError::new)?;

        if let Some(if_none_match) = if_none_match {
            if !if_none_match.precondition_passes(&session.etag()) {
                let mut response = Response::new(Bytes::new());
                *response.status_mut() = StatusCode::NOT_MODIFIED;
                prepare_headers(response.headers_mut(), session);
                http_response_to_twisted(twisted_request, response)?;
                return Ok(());
            }
        }

        let mut response = Response::new(session.data());
        *response.status_mut() = StatusCode::OK;
        let headers = response.headers_mut();
        prepare_headers(headers, session);
        headers.typed_insert(session.content_type());
        headers.typed_insert(session.content_length());
        http_response_to_twisted(twisted_request, response)?;

        Ok(())
    }

    fn handle_put(&mut self, twisted_request: &PyAny, id: &str) -> PyResult<()> {
        let request = http_request_from_twisted(twisted_request)?;

        let content_type = check_input_headers(request.headers())?;

        let if_match: IfMatch = request.headers().typed_get_required()?;

        let data = request.into_body();

        let id: Ulid = id.parse().map_err(|_| NotFoundError::new())?;
        let session = self.sessions.get_mut(&id).ok_or_else(NotFoundError::new)?;

        if !if_match.precondition_passes(&session.etag()) {
            let mut headers = HeaderMap::new();
            prepare_headers(&mut headers, session);

            let headers = headers
                .iter()
                .map(|(key, value)| {
                    (
                        key.as_str().to_owned(),
                        value
                            .to_str()
                            // XXX: will that ever panic?
                            .expect("header value is valid ASCII")
                            .to_owned(),
                    )
                })
                .collect::<HashMap<String, String>>();

            return Err(SynapseError::new_err((
                StatusCode::PRECONDITION_FAILED.as_u16(),
                "ETag does not match",
                "M_CONCURRENT_WRITE",
                None::<()>,
                headers,
            )));
        }

        session.update(data, content_type);

        let mut response = Response::new(Bytes::new());
        *response.status_mut() = StatusCode::ACCEPTED;
        prepare_headers(response.headers_mut(), session);
        http_response_to_twisted(twisted_request, response)?;

        Ok(())
    }

    fn handle_delete(&mut self, twisted_request: &PyAny, id: &str) -> PyResult<()> {
        let _request = http_request_from_twisted(twisted_request)?;

        let id: Ulid = id.parse().map_err(|_| NotFoundError::new())?;
        let _session = self.sessions.remove(&id).ok_or_else(NotFoundError::new)?;

        let mut response = Response::new(Bytes::new());
        *response.status_mut() = StatusCode::NO_CONTENT;
        response
            .headers_mut()
            .typed_insert(AccessControlAllowOrigin::ANY);
        http_response_to_twisted(twisted_request, response)?;

        Ok(())
    }
}

pub fn register_module(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    let child_module = PyModule::new(py, "rendezvous")?;

    child_module.add_class::<RendezVousHandler>()?;

    m.add_submodule(child_module)?;

    // We need to manually add the module to sys.modules to make `from
    // synapse.synapse_rust import rendezvous` work.
    py.import("sys")?
        .getattr("modules")?
        .set_item("synapse.synapse_rust.rendezvous", child_module)?;

    Ok(())
}
