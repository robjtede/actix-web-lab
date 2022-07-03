//! Experimental middleware.
//!
//! Analogous to the `middleware` module in Actix Web.

pub use crate::err_handler::ErrorHandlers;
pub use crate::middleware_from_fn::{from_fn, MiddlewareFn, Next};
pub use crate::normalize_path::NormalizePath;
pub use crate::redirect_to_https::RedirectHttps;
pub use crate::redirect_to_www::redirect_to_www;
