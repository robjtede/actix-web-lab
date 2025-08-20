# Changelog

## Unreleased

## 0.24.3

- Add `ConditionOption` middleware.

## 0.24.2

- Add `LazyDataShared` extractor.

## 0.24.1

- Fix `RedirectHttps` middleware when used with IPv6 addresses.

## 0.24.0

- Re-work `Json` extractor error handling.
- Re-work `UrlEncodedForm` extractor error handling.
- Upgrade to edition 2024.
- Minimum supported Rust version (MSRV) is now 1.85.

## 0.23.0

- Add `header::ClearSiteData` header.
- Add `header::ClearSiteDataDirective` type
- Remove `response::Html` responder.
- Remove `middleware::from_fn()` middleware.
- Remove `extract::ThinData` extractor.

## 0.22.0

- Add `extract::QueryDeserializeError` type.
- Re-work `Query` deserialization error handling.
- Implement `Clone` for `extract::Path<T: Clone>`.
- The `Deref` implementation for `header::CacheControl` now returns a slice instead of a `Vec`.
- Deprecate `middleware::from_fn()` now it has graduated to Actix Web.
- Deprecate `extract::ThinData` now it has graduated to Actix Web.

## 0.21.0

- Remove use of `async-trait` on `RequestSignatureScheme` trait.
- Deprecate `respond::Html` now it has graduated to Actix Web.

## 0.20.2

- Add `extract::ThinData` extractor.

## 0.20.1

- Add `redirect_to_non_www` fn middleware.

## 0.20.0

- Add `sse::Sse::from_infallible_stream()` method.
- Add `sse::Sse::{from_receiver, from_infallible_receiver}()` methods.
- Remove `sse::{Sender, ChannelStream}` types.
- Remove `sse::{SendError, TrySendError}` types.
- Remove `sse::channel()` function.
- Remove `sse::{SseSendError, SseTrySendError, SseData, SseMessage}` type aliases.
- Remove `web::Redirect` responder and `web::redirect()` function.
- Remove `guard::Acceptable` guard type.

## 0.19.2

- Add `extract::ReconstructedPath` extractor.
- Add `header::XForwardedPath` header.
- Expose `extract::DEFAULT_BODY_LIMIT`.

## 0.19.1

- Add `Host` extractor.

## 0.19.0

- Deprecate `web::Redirect` now it has graduated to Actix Web.
- Deprecate `guard::Acceptable` now it has graduated to Actix Web.
- Update `serde_html_form` dependency (which powers the `Query` and `Form` extractors) to `0.2`.
- Remove `spa` and `cbor` default crate features.

## 0.18.9

- Add `middleware::map_response()` for mapping responses with an async function.
- Add `middleware::map_response_body()` for mapping response bodies with an async function.
- Add `respond::{MessagePack,MessagePackNamed}` responders.
- Add `respond::Cbor` responder.

## 0.18.8

- Always add `Content-Encoding: identity` header when using `Sse` as a responder.

## 0.18.7

- Add `UrlEncodedForm` extractor with const generic size limit that supports collecting multi-value items into a `Vec`.

## 0.18.6

- Add `#[from_request(copy_from_app_data)]` field attribute to the `FromRequest` derive macro.
- Add extractor support to `from_fn` when used as initial arguments.
- Feature gate `spa` features. Enabled by default.
- Fix `derive` feature flag.

## 0.18.5

- Expose `util::fork_request_payload()` helper.
- Add reduced `Bytes` extractor with const generic size limit.

## 0.18.4

- Add `Forwarded` typed header.

## 0.18.3

- Fix `NormalizePath` when used in redirecting mode.

## 0.18.2

- Add `LoadShed` middleware.

## 0.18.1

- No significant changes since `0.18.0`.

## 0.18.0

- Add `body::{writer, Writer}` rudimentary async-write body adapter.
- Add error type parameter to `body::channel` and `body::Sender`.
- Add `body::Sender::close()` with optional error argument.
- Unwrap `body::Sender::send()` return error type.

## 0.17.0

- Add `sse::Sse::from_stream()` constructor.
- Add `sse::Data::new_json()` constructor.
- Rename `sse::{sse => channel}()`.
- Rename `sse::{SseData => Data}()`.
- Rename `sse::{SseMessage => Event}()`.
- Rename `sse::{SseSendError => SendError}`.
- Rename `sse::{SseTrySendError => TrySendError}`.
- Remove `extract::BodyHash` and associated types; promoted to `actix-hash` crate.
- Remove `extract::BodyHmac` and associated types; prefer `RequestSignatureScheme` + `RequestSignature`. See `body_hmac` in examples folder.
- Remove `header::Hsts` alias; use `header::StrictTransportSecurity`.

## 0.16.9

- Add `SseSender::send`.
- Add `SseSender::try_send`.
- Expose `SseData`.
- Expose `SseMessage`.
- Expose `SseSendError`.
- Deprecate other `SseSender` methods.

## 0.16.8

- Add semantic `Sse` (server-sent events) responder with channel-like interface.

## 0.16.7

- Implement `FromStr` and `Header::parse` for `StrictTransportSecurity`.
- Implement `PartialEq` and `Eq` for `StrictTransportSecurity`.

## 0.16.6

- Add `PanicReporter` middleware.

## 0.16.5

- Add basic `CatchPanic` middleware.

## 0.16.4

- Add alternative `NormalizePath` middleware with redirect option.

## 0.16.3

- Add `ContentLength` typed header.
- Add `BodyLimit` extractor wrapper.

## 0.16.2

- Rename `Hsts` header to `StrictTransportSecurity`. Old name kept as alias for compatibility.

## 0.16.1

- `Query` extractor now supports collecting multi-value items into a `Vec`.
- Add `derive` crate feature (on-by-default) that enables derive macros.

## 0.16.0

- Add very basic `FromRequest` derive macro.
- `RequestSignatureScheme` associated types are no longer bound to RustCrypto ecosystem.
- Deprecate `BodyHmac`; migrate to `RequestSignature[Scheme]`.

## 0.15.2

- No significant changes since `0.15.1`.

## 0.15.1

- Add `RequestSignatureScheme` trait and `RequestSignature` extractor.
- Add `SwapData` extractor.
- Add `LocalData` extractor.
- Deprecate `BodyHash`; it has migrated to the [`actix-hash`](https://crates.io/crates/actix-hash) crate.

## 0.15.0

- Add `CacheControl` typed header.
- Add `CacheDirective` type with support for modern cache directives.

## 0.14.0

- Add `test` module containing new test request builders and response testing macros.
- Add `RedirectHttps::to_port()` for specifying custom HTTPS redirect port.
- Fix `RedirectHttps` when host contains port already.

## 0.13.0

- Add `Hsts` (Strict-Transport-Security) typed header.
- Convert `redirect_to_https` function middleware to `RedirectHttps` middleware type.
- Add HSTS configuration to new `RedirectHttps` middleware.

## 0.12.1

- Add `Body{Hash, Hmac}::verify_slice()`.

## 0.12.0

- Add `Path` extractor that can be deconstructed.
- `Json` limit const generic parameter now has a default and can be omitted.
- Update `actix-web` dependency to `4.0.0`.

## 0.11.0

- Add alternate `ErrorHandler` middleware.
- Dynamic `HmacConfig` uses async function.
- `BodyHmac::into_parts` includes `Bytes` buffer.

## 0.10.0

- Add `Html` responder.
- Add `BodyHash` extractor wrapper.
- Add `BodyHmac` extractor wrapper.

## 0.9.0

- Add quick SPA service builder `web::spa()`.
- Copy `Query` extractor from Actix Web that can track minor versions of `serde-urlencoded`.

## 0.8.0

- `Csv`, `NdJson`, and `DisplayStream` now take fallible streams.
- Add `{Csv, NdJson, DisplayStream}::new_infallible`.

## 0.7.1

- Add `Redirect::permanent`.
- Default `Redirect` status is now 307 (temporary redirect).

## 0.7.0

- Add `channel` body type.
- `from_fn` middleware can now alter the body type.
- `Next<B>` has an inherent `call` method so that the `Service` doesn't need importing.

## 0.6.1

- No significant changes since `0.6.0`.

## 0.6.0

- Add `DisplayStream` responder.
- Add `from_fn` middleware.

## 0.5.0

- Organise modules and exports.

## 0.4.0

- Add `Csv` responder.

## 0.3.0

- Add `NdJson` responder.

## 0.2.3

- No significant changes since `0.2.2`.

## 0.2.2

- Fix exports.
- Exclude default Actix Web features.

## 0.2.1

- Add `LazyData` extractor.

## 0.2.0

- Add reduced `Json` extractor with const generic size limit.
- Add `Redirect` service.
- Add `Acceptable` guard.

# 0.1.0

- Empty crate.
