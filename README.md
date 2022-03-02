# actix-web-lab

> Experimental extractors, middleware, and other extras for possible inclusion in Actix Web.

[![crates.io](https://img.shields.io/crates/v/actix-web-lab?label=latest)](https://crates.io/crates/actix-web-lab)
[![Documentation](https://docs.rs/actix-web-lab/badge.svg)](https://docs.rs/actix-web-lab/0.12.1)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/actix-web-lab.svg)
<br />
[![dependency status](https://deps.rs/crate/actix-web-lab/0.12.1/status.svg)](https://deps.rs/crate/actix-web-lab/0.12.1)
[![Download](https://img.shields.io/crates/d/actix-web-lab.svg)](https://crates.io/crates/actix-web-lab)
[![CircleCI](https://circleci.com/gh/robjtede/actix-web-lab/tree/main.svg?style=shield)](https://circleci.com/gh/robjtede/actix-web-lab/tree/main)

## Features
**[Feature Voting &rarr;](https://github.com/robjtede/actix-web-lab/discussions/7)**

### Responders
- `Csv`: efficient CSV streaming [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/respond/struct.Csv.html)
- `NdJson`: efficient NDJSON streaming [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/respond/struct.NdJson.html)
- `DisplayStream`: efficient line-by-line `Display` streaming [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/respond/struct.DisplayStream.html)
- `Html`: basic string wrapper that responds with HTML Content-Type [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/respond/struct.Html.html)

### Middleware
- `from_fn`: use an async function as a middleware [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/middleware/fn.from_fn.html)
- `redirect_to_https`: function middleware to redirect traffic to HTTPS if connection is insecure [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/middleware/fn.redirect_to_https.html)
- `redirect_to_www`: function middleware to redirect traffic to `www.` if not already there [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/middleware/fn.redirect_to_www.html)
- `ErrorHandlers`: alternative error handler middleware with simpler interface [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/middleware/struct.ErrorHandlers.html)

### Extractors
- `LazyData`: app data/state initialized on first use [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/extract/struct.LazyData.html)
- `Json`: simplified JSON extractor with const-generic limits [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/extract/struct.Json.html)
- `Path`: simplified path parameter extractor that supports destructuring [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/extract/struct.Path.html)
- `Query`: simplified query-string extractor [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/extract/struct.Query.html)
- `BodyHash`: wraps an extractor and calculates a body checksum hash alongside [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/extract/struct.BodyHash.html)
- `BodyHmac`: wraps an extractor and calculates a body HMAC alongside [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/extract/struct.BodyHmac.html)

### Body Types
- `channel`: a simple channel-like body type with a sender side that can be used from another thread [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/body/fn.channel.html)

### Services
- `Redirect`: simple redirects [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/web/struct.Redirect.html)
- `spa`: Easy Single-page Application (SPA) service [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/web/fn.spa.html)

### Route Guards
- `Acceptable`: verifies that an `Accept` header is present and it contains a compatible MIME type [(docs)](https://docs.rs/actix-web-lab/0.12.1/actix_web_lab/guard/struct.Acceptable.html)


## Things To Know About This Crate

- It will never reach v1.0.
- Minimum Supported Rust Version (MSRV) is latest stable at the time of each release.
- Breaking changes will likely happen on most 0.x version bumps.
- Documentation might be limited for some items.
- Items that graduate to Actix Web crate will be marked deprecated here for a reasonable amount of time so you can migrate.
- Migrating will often be as easy as dropping the `_lab` suffix from imports when migrating.
