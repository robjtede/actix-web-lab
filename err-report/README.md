# err-report

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/err-report?label=latest)](https://crates.io/crates/err-report)
[![Documentation](https://docs.rs/err-report/badge.svg?version=0.0.2)](https://docs.rs/err-report/0.0.2)
![Version](https://img.shields.io/badge/rustc-1.75+-ab6000.svg)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/err-report.svg)
<br />
[![Dependency Status](https://deps.rs/crate/err-report/0.0.2/status.svg)](https://deps.rs/crate/err-report/0.0.2)
[![Download](https://img.shields.io/crates/d/err-report.svg)](https://crates.io/crates/err-report)

<!-- prettier-ignore-end -->

<!-- cargo-rdme start -->

Clone of the unstable [`err_report::Report`] type.

Backtrace support is omitted due to nightly requirement.

Copied on 2025-09-09.

## Examples

```rust
use err_report::Report;

let err = std::io::Error::new(
std::io::ErrorKind::InvalidData,
std::io::Error::new(
    std::io::ErrorKind::InvalidData,
std::io::Error::other("Invalid file name"),
)));

assert_eq!(
    "Failed to connect",
    Report::new(err).to_string(),
);
```

<!-- cargo-rdme end -->
