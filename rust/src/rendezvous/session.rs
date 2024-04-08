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

use std::time::{Duration, SystemTime};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use bytes::Bytes;
use headers::{ContentLength, ContentType, ETag, Expires, LastModified};
use mime::Mime;
use sha2::{Digest, Sha256};

pub struct Session {
    hash: [u8; 32],
    data: Bytes,
    content_type: Mime,
    last_modified: SystemTime,
    expires: SystemTime,
}

impl Session {
    pub fn new(data: Bytes, content_type: Mime, ttl: Duration) -> Self {
        let hash = Sha256::digest(&data).into();
        let now = SystemTime::now();
        Self {
            hash,
            data,
            content_type,
            expires: now + ttl,
            last_modified: now,
        }
    }

    pub fn update(&mut self, data: Bytes, content_type: Mime) {
        self.hash = Sha256::digest(&data).into();
        self.data = data;
        self.content_type = content_type;
        self.last_modified = SystemTime::now();
    }

    pub fn content_type(&self) -> ContentType {
        self.content_type.clone().into()
    }

    pub fn content_length(&self) -> ContentLength {
        ContentLength(self.data.len() as _)
    }

    pub fn etag(&self) -> ETag {
        let encoded = URL_SAFE_NO_PAD.encode(self.hash);
        // SAFETY: Base64 encoding is URL-safe, so ETag-safe
        format!("\"{encoded}\"")
            .parse()
            .expect("base64-encoded hash should be URL-safe")
    }

    pub fn data(&self) -> Bytes {
        self.data.clone()
    }

    pub fn last_modified(&self) -> LastModified {
        self.last_modified.into()
    }

    pub fn expires(&self) -> Expires {
        self.expires.into()
    }
}
