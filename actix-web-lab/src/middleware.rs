//! Experimental middleware.
//!
//! Analogous to the `middleware` module in Actix Web.

pub use crate::{
    catch_panic::CatchPanic,
    err_handler::ErrorHandlers,
    load_shed::LoadShed,
    middleware_map_response::{MapResMiddleware, map_response},
    middleware_map_response_body::{MapResBodyMiddleware, map_response_body},
    normalize_path::NormalizePath,
    panic_reporter::PanicReporter,
    redirect_to_https::RedirectHttps,
    redirect_to_non_www::redirect_to_non_www,
    redirect_to_www::redirect_to_www,
};
