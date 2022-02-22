//! Experimental middleware.
//!
//! Analogous to the `middleware` module in Actix Web.

pub use crate::err_handler::ErrorHandlers;
pub use crate::middleware_from_fn::{from_fn, MiddlewareFn, Next};
pub use crate::redirect_to_https::redirect_to_https;
pub use crate::redirect_to_www::redirect_to_www;
