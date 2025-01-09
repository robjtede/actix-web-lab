//! MessagePack responder.

use std::sync::LazyLock;

use actix_web::{HttpRequest, HttpResponse, Responder};
use bytes::Bytes;
use derive_more::Display;
use mime::Mime;
use serde::Serialize;

static MSGPACK_MIME: LazyLock<Mime> = LazyLock::new(|| "application/msgpack".parse().unwrap());

/// [MessagePack] responder.
///
/// If you require the fields to be named, use [`MessagePackNamed`].
///
/// [MessagePack]: https://msgpack.org/
#[derive(Debug, Display)]
pub struct MessagePack<T>(pub T);

impl_more::impl_deref_and_mut!(<T> in MessagePack<T> => T);

impl<T: Serialize> Responder for MessagePack<T> {
    type Body = Bytes;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        let body = Bytes::from(rmp_serde::to_vec(&self.0).unwrap());

        HttpResponse::Ok()
            .content_type(MSGPACK_MIME.clone())
            .message_body(body)
            .unwrap()
    }
}

/// MessagePack responder with named fields.
#[derive(Debug, Display)]
pub struct MessagePackNamed<T>(pub T);

impl_more::impl_deref_and_mut!(<T> in MessagePackNamed<T> => T);

impl<T: Serialize> Responder for MessagePackNamed<T> {
    type Body = Bytes;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        let body = Bytes::from(rmp_serde::to_vec_named(&self.0).unwrap());

        HttpResponse::Ok()
            .content_type(MSGPACK_MIME.clone())
            .message_body(body)
            .unwrap()
    }
}
