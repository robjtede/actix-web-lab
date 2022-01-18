//! In-progress extractors and middleware for Actix Web.
//!
//! # What Is This Crate?
//! This crate serves as a preview and test ground for upcoming features and ideas for Actix Web's
//! built in library of extractors, middleware and other utilities.
//!
//! # Things To Know About This Crate
//! - It will never reach v1.0.
//! - Minimum Supported Rust Version (MSRV) is latest stable at the time of each release.
//! - Breaking changes will likely happen on every 0.x version bump.
//! - Documentation will probably be limited for some items.
//! - Items that graduate to the main Actix Web crate will be deprecated for at least one minor
//!   version of this crate.
//! - It will often be as easy as dropping the `_lab` suffix from imports when items graduate.

mod acceptable;
mod buffered_serializing_stream;
mod csv;
mod display_stream;
mod json;
mod lazy_data;
mod middleware_from_fn;
mod ndjson;
mod redirect;
mod utils;

// public API
pub mod extract;
pub mod guard;
pub mod middleware;
pub mod respond;
pub mod web;
