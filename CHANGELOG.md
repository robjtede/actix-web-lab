# Changelog

## Unreleased - 2022-xx-xx
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
- Added `{Csv, NdJson, DisplayStream}::new_infallible`.


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
- Added `LazyData` extractor.

## 0.2.0 - 2022-01-04
- Added reduced `Json` extractor with const generic size limit.
- Added `Redirect` service.
- Added `Acceptable` guard.

# 0.1.0
- Empty crate.
