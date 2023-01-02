//! MessagePack responder.

use actix_web::{HttpRequest, HttpResponse, Responder};
use bytes::Bytes;
use derive_more::{Deref, DerefMut, Display};
use mime::Mime;
use once_cell::sync::Lazy;
use serde::Serialize;

static MSGPACK_MIME: Lazy<Mime> = Lazy::new(|| "application/msgpack".parse().unwrap());

/// MessagePack responder.
#[cfg_attr(docsrs, doc(cfg(feature = "msgpack")))]
#[derive(Debug, Deref, DerefMut, Display)]
pub struct MessagePack<T>(pub T);

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
#[cfg_attr(docsrs, doc(cfg(feature = "msgpack")))]
#[derive(Debug, Deref, DerefMut, Display)]
pub struct MessagePackNamed<T>(pub T);

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
