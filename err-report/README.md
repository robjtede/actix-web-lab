# err-report

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/err-report?label=latest)](https://crates.io/crates/err-report)
[![Documentation](https://docs.rs/err-report/badge.svg?version=0.1.0)](https://docs.rs/err-report/0.1.0)
![Version](https://img.shields.io/badge/rustc-1.75+-ab6000.svg)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/err-report.svg)
<br />
[![Dependency Status](https://deps.rs/crate/err-report/0.1.0/status.svg)](https://deps.rs/crate/err-report/0.1.0)
[![Download](https://img.shields.io/crates/d/err-report.svg)](https://crates.io/crates/err-report)

<!-- prettier-ignore-end -->

<!-- cargo-rdme start -->

Clone of the unstable [`err_report::Report`] type.

Backtrace support is omitted due to nightly requirement.

Copied on 2025-09-09.

## Examples

```rust
use std::ffi::CString;
use err_report::Report;

let invalid_utf8 = vec![b'f', 0xff, b'o', b'o'];
let cstring = CString::new(invalid_utf8).unwrap();
let err = cstring.into_string().err().unwrap();

// without Report
assert_eq!(
    "invalid utf-8 sequence of 1 bytes from index 1",
    err.utf8_error().to_string(),
);
assert_eq!(
    "C string contained non-utf8 bytes",
    err.to_string(),
);

// with Report
assert_eq!(
    "invalid utf-8 sequence of 1 bytes from index 1",
    Report::new(err.utf8_error()).to_string(),
);
assert_eq!(
    "C string contained non-utf8 bytes: invalid utf-8 sequence of 1 bytes from index 1",
    Report::new(err).to_string(),
);
```

<!-- cargo-rdme end -->
