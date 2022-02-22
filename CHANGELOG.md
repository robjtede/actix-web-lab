# Changelog

## Unreleased - 2021-xx-xx
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
