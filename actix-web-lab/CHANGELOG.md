# Changelog

## Unreleased - 2022-xx-xx

- Add `UrlEncodedForm` extractor with const generic size limit that supports collecting multi-value items into a `Vec`.

## 0.18.6 - 2022-12-08

- Add `#[from_request(copy_from_app_data)]` field attribute to the `FromRequest` derive macro.
- Add extractor support to `from_fn` when used as initial arguments.
- Feature gate `spa` features. Enabled by default.
- Fix `derive` feature flag.

## 0.18.5 - 2022-10-13

- Expose `util::fork_request_payload()` helper.
- Add reduced `Bytes` extractor with const generic size limit.

## 0.18.4 - 2022-10-09

- Add `Forwarded` typed header.

## 0.18.3 - 2022-10-01

- Fix `NormalizePath` when used in redirecting mode.

## 0.18.2 - 2022-09-13

- Add `LoadShed` middleware.

## 0.18.1 - 2022-09-13

- No significant changes since `0.18.0`.

## 0.18.0 - 2022-09-13

- Add `body::{writer, Writer}` rudimentary async-write body adapter.
- Add error type parameter to `body::channel` and `body::Sender`.
- Add `body::Sender::close()` with optional error argument.
- Unwrap `body::Sender::send()` return error type.

## 0.17.0 - 2022-08-09

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

## 0.16.9 - 2022-08-07

- Add `SseSender::send`.
- Add `SseSender::try_send`.
- Expose `SseData`.
- Expose `SseMessage`.
- Expose `SseSendError`.
- Deprecate other `SseSender` methods.

## 0.16.8 - 2022-08-07

- Add semantic `Sse` (server-sent events) responder with channel-like interface.

## 0.16.7 - 2022-07-27

- Implement `FromStr` and `Header::parse` for `StrictTransportSecurity`.
- Implement `PartialEq` and `Eq` for `StrictTransportSecurity`.

## 0.16.6 - 2022-07-27

- Add `PanicReporter` middleware.

## 0.16.5 - 2022-07-24

- Add basic `CatchPanic` middleware.

## 0.16.4 - 2022-07-03

- Add alternative `NormalizePath` middleware with redirect option.

## 0.16.3 - 2022-07-03

- Add `ContentLength` typed header.
- Add `BodyLimit` extractor wrapper.

## 0.16.2 - 2022-07-02

- Rename `Hsts` header to `StrictTransportSecurity`. Old name kept as alias for compatibility.

## 0.16.1 - 2022-04-23

- `Query` extractor now supports collecting multi-value items into a `Vec`.
- Add `derive` crate feature (on-by-default) that enables derive macros.

## 0.16.0 - 2022-04-11

- Add very basic `FromRequest` derive macro.
- `RequestSignatureScheme` associated types are no longer bound to RustCrypto ecosystem.
- Deprecate `BodyHmac`; migrate to `RequestSignature[Scheme]`.

## 0.15.2 - 2022-04-08

- No significant changes since `0.15.1`.

## 0.15.1 - 2022-04-07

- Add `RequestSignatureScheme` trait and `RequestSignature` extractor.
- Add `SwapData` extractor.
- Add `LocalData` extractor.
- Deprecate `BodyHash`; it has migrated to the [`actix-hash`](https://crates.io/crates/actix-hash) crate.

## 0.15.0 - 2022-03-07

- Add `CacheControl` typed header.
- Add `CacheDirective` type with support for modern cache directives.

## 0.14.0 - 2022-03-07

- Add `test` module containing new test request builders and response testing macros.
- Add `RedirectHttps::to_port()` for specifying custom HTTPS redirect port.
- Fix `RedirectHttps` when host contains port already.

## 0.13.0 - 2022-03-03

- Add `Hsts` (Strict-Transport-Security) typed header.
- Convert `redirect_to_https` function middleware to `RedirectHttps` middleware type.
- Add HSTS configuration to new `RedirectHttps` middleware.

## 0.12.1 - 2022-03-02

- Add `Body{Hash, Hmac}::verify_slice()`.

## 0.12.0 - 2022-02-25

- Add `Path` extractor that can be deconstructed.
- `Json` limit const generic parameter now has a default and can be omitted.
- Update `actix-web` dependency to `4.0.0`.

## 0.11.0 - 2022-02-22

- Add alternate `ErrorHandler` middleware.
- Dynamic `HmacConfig` uses async function.
- `BodyHmac::into_parts` includes `Bytes` buffer.

## 0.10.0 - 2022-02-07

- Add `Html` responder.
- Add `BodyHash` extractor wrapper.
- Add `BodyHmac` extractor wrapper.

## 0.9.0 - 2022-01-22

- Add quick SPA service builder `web::spa()`.
- Copy `Query` extractor from Actix Web that can track minor versions of `serde-urlencoded`.

## 0.8.0 - 2022-01-20

- `Csv`, `NdJson`, and `DisplayStream` now take fallible streams.
- Add `{Csv, NdJson, DisplayStream}::new_infallible`.

## 0.7.1 - 2022-01-19

- Add `Redirect::permanent`.
- Default `Redirect` status is now 307 (temporary redirect).

## 0.7.0 - 2022-01-18

- Add `channel` body type.
- `from_fn` middleware can now alter the body type.
- `Next<B>` has an inherent `call` method so that the `Service` doesn't need importing.

## 0.6.1 - 2022-01-18

- No significant changes since `0.6.0`.

## 0.6.0 - 2022-01-18

- Add `DisplayStream` responder.
- Add `from_fn` middleware.

## 0.5.0 - 2022-01-18

- Organise modules and exports.

## 0.4.0 - 2022-01-18

- Add `Csv` responder.

## 0.3.0 - 2022-01-17

- Add `NdJson` responder.

## 0.2.3 - 2022-01-14

- No significant changes since `0.2.2`.

## 0.2.2 - 2022-01-05

- Fix exports.
- Exclude default Actix Web features.

## 0.2.1 - 2022-01-05

- Add `LazyData` extractor.

## 0.2.0 - 2022-01-04

- Add reduced `Json` extractor with const generic size limit.
- Add `Redirect` service.
- Add `Acceptable` guard.

# 0.1.0

- Empty crate.
