# `truncate-safe`

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/truncate-safe?label=latest)](https://crates.io/crates/truncate-safe)
[![Documentation](https://docs.rs/truncate-safe/badge.svg)](https://docs.rs/truncate-safe)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/truncate-safe.svg)
![Version](https://img.shields.io/badge/rustc-1.88+-ab6000.svg)

<!-- prettier-ignore-end -->

<!-- cargo-rdme start -->

Truncate strings at UTF-8 character boundaries with an optional ellipsis.

This crate is `no_std` and uses `alloc` for owned results.

The limit is a byte count. If the limit is inside a UTF-8 character, truncation stops before that character. Limits at or above the input length leave the text unchanged.

An ellipsis is appended only when text is removed. Its bytes are not included in the limit. Truncation preserves UTF-8 characters but can split a grapheme cluster, such as a letter and its combining accent.

## Examples

```rust
use truncate_safe::{truncate, truncate_with, truncate_with_ellipsis};

assert_eq!(truncate("Hello, 世界!", 9), "Hello, ");
assert_eq!(truncate_with_ellipsis("Hello, 世界!", 9), "Hello, …");
assert_eq!(truncate_with_ellipsis("Hello", 5), "Hello");
assert_eq!(truncate_with("Hello, 世界!", 9, "..."), "Hello, ...");
```

<!-- cargo-rdme end -->
