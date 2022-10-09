//! In-progress extractors and middleware for Actix Web.
//!
//! # What Is This Crate?
//! This crate serves as a preview and test ground for upcoming features and ideas for Actix Web's
//! built in library of extractors, middleware and other utilities.
//!
//! Any kind of feedback is welcome.
//!
//! # Complete Examples
//! See [the `examples` folder][examples] for some complete examples of items in this crate.
//!
//! # Things To Know About This Crate
//! - It will never reach v1.0.
//! - Minimum Supported Rust Version (MSRV) is latest stable at the time of each release.
//! - Breaking changes will likely happen on most 0.x version bumps.
//! - Documentation might be limited for some items.
//! - Items that graduate to Actix Web crate will be marked deprecated here for a reasonable amount
//!   of time so you can migrate.
//! - Migrating will often be as easy as dropping the `_lab` suffix from imports when migrating.
//!
//! [examples]: https://github.com/robjtede/actix-web-lab/tree/HEAD/actix-web-lab/examples

#![deny(rust_2018_idioms, nonstandard_style)]
#![warn(future_incompatible, missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod acceptable;
mod body_async_write;
mod body_channel;
mod body_extractor_fold;
mod body_limit;
mod cache_control;
mod catch_panic;
mod content_length;
mod csv;
mod display_stream;
mod err_handler;
mod forwarded;
mod html;
mod infallible_body_stream;
mod json;
mod lazy_data;
mod load_shed;
mod local_data;
mod middleware_from_fn;
mod ndjson;
mod normalize_path;
mod panic_reporter;
mod path;
mod query;
mod redirect;
mod redirect_to_https;
mod redirect_to_www;
mod request_signature;
mod spa;
mod strict_transport_security;
mod swap_data;
#[cfg(test)]
mod test_header_macros;
mod test_request_macros;
mod test_response_macros;
mod test_services;

// public API
pub mod body;
pub mod extract;
pub mod guard;
pub mod header;
pub mod middleware;
pub mod respond;
pub mod sse;
pub mod test;
pub mod util;
pub mod web;

pub use actix_web_lab_derive::FromRequest;

// private re-exports for macros
#[doc(hidden)]
pub mod __reexports {
    pub use ::actix_web;
    pub use ::futures_util;
    pub use ::serde_json;
    pub use ::tokio;
}

pub(crate) type BoxError = Box<dyn std::error::Error>;
