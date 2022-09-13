//! Experimental body types.
//!
//! Analogous to the `body` module in Actix Web.

pub use crate::{
    body_async_write::{writer, Writer},
    body_channel::{channel, Sender},
    infallible_body_stream::{new_infallible_body_stream, new_infallible_sized_stream},
};
