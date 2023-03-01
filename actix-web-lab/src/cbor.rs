//! CBOR responder.

use actix_web::{HttpRequest, HttpResponse, Responder};
use bytes::Bytes;
use derive_more::{Deref, DerefMut, Display};
use mime::Mime;
use once_cell::sync::Lazy;
use serde::Serialize;

static CBOR_MIME: Lazy<Mime> = Lazy::new(|| "application/cbor".parse().unwrap());

/// CBOR responder.
#[derive(Debug, Deref, DerefMut, Display)]
pub struct Cbor<T>(pub T);

impl<T: Serialize> Responder for Cbor<T> {
    type Body = Bytes;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        let body = Bytes::from(serde_cbor_2::to_vec(&self.0).unwrap());

        HttpResponse::Ok()
            .content_type(CBOR_MIME.clone())
            .message_body(body)
            .unwrap()
    }
}
