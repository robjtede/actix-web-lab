//! Experimental body types.
//!
//! Analogous to the `body` module in Actix Web.

pub use crate::{
    body_async_write::{Writer, writer},
    body_channel::{Sender, channel},
    infallible_body_stream::{new_infallible_body_stream, new_infallible_sized_stream},
};
