//! Experimental body types.
//!
//! Analogous to the `body` module in Actix Web.

pub use crate::channel_body::{channel, Sender};
pub use crate::infallible_body_stream::{new_infallible_body_stream, new_infallible_sized_stream};
