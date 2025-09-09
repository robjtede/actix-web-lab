# err-report

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/err-report?label=latest)](https://crates.io/crates/err-report)
[![Documentation](https://docs.rs/err-report/badge.svg?version=0.1.2)](https://docs.rs/err-report/0.1.2)
![Version](https://img.shields.io/badge/rustc-1.75+-ab6000.svg)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/err-report.svg)
<br />
[![Dependency Status](https://deps.rs/crate/err-report/0.1.2/status.svg)](https://deps.rs/crate/err-report/0.1.2)
[![Download](https://img.shields.io/crates/d/err-report.svg)](https://crates.io/crates/err-report)

<!-- prettier-ignore-end -->

<!-- cargo-rdme start -->

Clone of the unstable [`std::error::Report`] type.

Backtrace support is omitted due to nightly requirement.

Copied on 2025-09-09.

## Examples

```rust
use std::ffi::CString;

use err_report::Report;

let invalid_utf8 = [b'f', 0xff, b'o', b'o'];
let c_string = CString::new(invalid_utf8).unwrap();
let err = c_string.into_string().unwrap_err();

// without Report, the source/root error is not printed
assert_eq!("C string contained non-utf8 bytes", err.to_string());

// with Report, all details in error chain are printed
assert_eq!(
    "C string contained non-utf8 bytes: invalid utf-8 sequence of 1 bytes from index 1",
    Report::new(err).to_string(),
);
```

<!-- cargo-rdme end -->
