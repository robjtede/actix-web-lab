//! Truncate strings at UTF-8 character boundaries with an optional ellipsis.
//!
//! This crate is `no_std` and uses `alloc` for owned results.
//!
//! The limit is a byte count. If the limit is inside a UTF-8 character, truncation stops before that
//! character. Limits at or above the input length leave the text unchanged.
//!
//! An ellipsis is appended only when text is removed. Its bytes are not included in the limit.
//! Truncation preserves UTF-8 characters but can split a grapheme cluster, such as a letter and its
//! combining accent.
//!
//! # Examples
//!
//! ```rust
//! use truncate_safe::{truncate, truncate_with, truncate_with_ellipsis};
//!
//! assert_eq!(truncate("Hello, 世界!", 9), "Hello, ");
//! assert_eq!(truncate_with_ellipsis("Hello, 世界!", 9), "Hello, …");
//! assert_eq!(truncate_with_ellipsis("Hello", 5), "Hello");
//! assert_eq!(truncate_with("Hello, 世界!", 9, "..."), "Hello, ...");
//! ```

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::borrow::{Cow, ToOwned};

/// Returns the longest prefix whose byte length does not exceed `max_len`.
///
/// If `max_len` is inside a UTF-8 character, the prefix ends before that character. If `max_len` is
/// at or above the input length, returns the full input. A zero limit returns an empty slice.
///
/// The result borrows from `text`. This function does not allocate.
///
/// # Examples
///
/// ```rust
/// use truncate_safe::truncate;
///
/// assert_eq!(truncate("café", 4), "caf");
/// assert_eq!(truncate("café", 5), "café");
/// assert_eq!(truncate("café", usize::MAX), "café");
/// ```
#[must_use]
pub fn truncate(text: &str, max_len: usize) -> &str {
    let mut end = max_len.min(text.len());

    // A UTF-8 character has at most three continuation bytes.
    for _ in 0..3 {
        if text.is_char_boundary(end) {
            break;
        }

        end -= 1;
    }

    &text[..end]
}

/// Truncates `text` and appends the Unicode ellipsis (`…`) only when text is removed.
///
/// Equivalent to [`truncate_with`] with `"…"`. The ellipsis bytes are not included in `max_len`.
/// Returns a borrowed result when the input fits within the limit. Otherwise, returns an owned
/// string.
///
/// # Examples
///
/// ```rust
/// use truncate_safe::truncate_with_ellipsis;
///
/// assert_eq!(truncate_with_ellipsis("café", 4), "caf…");
/// assert_eq!(truncate_with_ellipsis("café", 5), "café");
/// assert_eq!(truncate_with_ellipsis("café", 0), "…");
/// ```
pub fn truncate_with_ellipsis(text: &str, max_len: usize) -> Cow<'_, str> {
    truncate_with(text, max_len, "…")
}

/// Truncates `text` and appends a custom `ellipsis` only when text is removed.
///
/// The prefix follows the same byte limit and UTF-8 rules as [`truncate`]. The ellipsis is appended
/// after truncation, so its bytes are not included in `max_len`. The result can exceed `max_len`.
/// A zero limit with nonempty input returns only the ellipsis.
///
/// Returns a borrowed result when the input fits within the limit or `ellipsis` is empty.
/// Otherwise, returns an owned string. The ellipsis can be any string, such as `"…"` or `"..."`.
///
/// # Examples
///
/// ```rust
/// use truncate_safe::truncate_with;
///
/// assert_eq!(truncate_with("café", 4, "…"), "caf…");
/// assert_eq!(truncate_with("café", 4, "..."), "caf...");
/// assert_eq!(truncate_with("café", 5, "..."), "café");
/// ```
pub fn truncate_with<'a>(text: &'a str, max_len: usize, ellipsis: &str) -> Cow<'a, str> {
    let prefix = truncate(text, max_len);

    if prefix.len() == text.len() || ellipsis.is_empty() {
        return Cow::Borrowed(prefix);
    }

    let mut result = prefix.to_owned();
    result.push_str(ellipsis);

    Cow::Owned(result)
}
