//! Experimental middleware.
//!
//! Analogous to the `middleware` module in Actix Web.

pub use crate::{
    catch_panic::CatchPanic,
    err_handler::ErrorHandlers,
    load_shed::LoadShed,
    middleware_from_fn::{from_fn, MiddlewareFn, Next},
    normalize_path::NormalizePath,
    panic_reporter::PanicReporter,
    redirect_to_https::RedirectHttps,
    redirect_to_www::redirect_to_www,
};
