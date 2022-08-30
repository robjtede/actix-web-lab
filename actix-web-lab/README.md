# actix-web-lab

> Experimental extractors, middleware, and other extras for possible inclusion in Actix Web.

[![crates.io](https://img.shields.io/crates/v/actix-web-lab?label=latest)](https://crates.io/crates/actix-web-lab)
[![Documentation](https://docs.rs/actix-web-lab/badge.svg)](https://docs.rs/actix-web-lab/0.17.0)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/actix-web-lab.svg)
<br />
[![dependency status](https://deps.rs/crate/actix-web-lab/0.17.0/status.svg)](https://deps.rs/crate/actix-web-lab/0.17.0)
[![Download](https://img.shields.io/crates/d/actix-web-lab.svg)](https://crates.io/crates/actix-web-lab)
[![CircleCI](https://circleci.com/gh/robjtede/actix-web-lab/tree/main.svg?style=shield)](https://circleci.com/gh/robjtede/actix-web-lab/tree/main)

## Features

**[Feature Voting &rarr;](https://github.com/robjtede/actix-web-lab/discussions/7)**

### Responders

- `Csv`: efficient CSV streaming [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/respond/struct.Csv.html)
- `NdJson`: efficient NDJSON streaming [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/respond/struct.NdJson.html)
- `DisplayStream`: efficient line-by-line `Display` streaming [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/respond/struct.DisplayStream.html)
- `Html`: basic string wrapper that responds with HTML Content-Type [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/respond/struct.Html.html)
- `Sse`: semantic server-sent events (SSE) responder with a channel-like interface [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/sse/index.html)

### Middleware

- `from_fn`: use an async function as a middleware [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/fn.from_fn.html)
- `RedirectHttps`: middleware to redirect traffic to HTTPS if connection is insecure with optional HSTS [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/struct.RedirectHttps.html)
- `redirect_to_www`: function middleware to redirect traffic to `www.` if not already there [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/fn.redirect_to_www.html)
- `ErrorHandlers`: alternative error handler middleware with simpler interface [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/struct.ErrorHandlers.html)
- `NormalizePath`: alternative path normalizing middleware with redirect option [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/struct.NormalizePath.html)
- `CatchPanic`: catch panics in wrapped handlers and middleware, returning empty 500 responses [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/struct.CatchPanic.html)
- `PanicReporter`: catch panics in wrapped handlers and middleware, returning empty 500 responses [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/middleware/struct.PanicReporter.html)


### Extractors

- `LazyData`: app data/state initialized on first use [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.LazyData.html)
- `SwapData`: app data/state that can be replaced at runtime (alternative to `Data<RwLock<T>>`) [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.SwapData.html)
- `LocalData`: app data/state that uses an `Rc` internally, avoiding atomic overhead (alternative to `Data<RwLock<T>>`) [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.LocalData.html)
- `Json`: simplified JSON extractor with const-generic limits [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.Json.html)
- `Path`: simplified path parameter extractor that supports destructuring [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.Path.html)
- `Query`: simplified query-string extractor that can also collect multi-value items [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.Query.html)
- `RequestSignature`: wraps an extractor and calculates a request signature alongside [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.RequestSignature.html)
- `BodyLimit`: wraps a body extractor and prevents DoS attacks by limiting payload size [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/extract/struct.BodyLimit.html)

### Macros

- `FromRequest`: Derive macro to implement `FromRequest` on an aggregate struct of other extractors [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/derive.FromRequest.html)

### Headers

- `StrictTransportSecurity`: Strict-Transport-Security (HSTS) configuration [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/header/struct.StrictTransportSecurity.html)
- `CacheControl`: Cache-Control typed header with support for modern directives [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/header/struct.CacheControl.html)
- `ContentLength`: Content-Length typed header [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/header/struct.ContentLength.html)

### Body Types

- `channel`: a simple channel-like body type with a sender side that can be used from another thread [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/body/fn.channel.html)

### Services

- `Redirect`: simple redirects [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/web/struct.Redirect.html)
- `spa`: Easy Single-page Application (SPA) service [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/web/fn.spa.html)

### Route Guards

- `Acceptable`: verifies that an `Accept` header is present and it contains a compatible MIME type [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/guard/struct.Acceptable.html)

### Test Utilities

- `test_request`: Construct `TestRequest` using an HTTP-like DSL [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/test/macro.test_request.html)
- `assert_response_matches`: quickly write tests that check various parts of a `ServiceResponse` [(docs)](https://docs.rs/actix-web-lab/0.17.0/actix_web_lab/test/macro.assert_response_matches.html)

## Things To Know About This Crate

- It will never reach v1.0.
- Minimum Supported Rust Version (MSRV) is latest stable at the time of each release.
- Breaking changes will likely happen on most 0.x version bumps.
- Documentation might be limited for some items.
- Items that graduate to Actix Web crate will be marked deprecated here for a reasonable amount of time so you can migrate.
- Migrating will often be as easy as dropping the `_lab` suffix from imports.
