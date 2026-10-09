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

#[cfg(test)]
mod tests {
    use alloc::borrow::{Cow, ToOwned};

    use super::{truncate, truncate_with, truncate_with_ellipsis};

    #[test]
    fn truncates_ascii_at_the_byte_limit() {
        assert_eq!(truncate("hello", 3), "hel");
    }

    #[test]
    fn rounds_down_at_each_utf8_character_width() {
        let text = "aé中🦀z";
        let prefixes = [
            "",
            "a",
            "a",
            "aé",
            "aé",
            "aé",
            "aé中",
            "aé中",
            "aé中",
            "aé中",
            "aé中🦀",
            text,
        ];

        for (max_len, expected) in prefixes.into_iter().enumerate() {
            assert_eq!(truncate(text, max_len), expected, "byte limit: {max_len}");
        }
    }

    #[test]
    fn leaves_text_unchanged_when_it_fits() {
        for max_len in [5, 6, usize::MAX] {
            assert_eq!(truncate("café", max_len), "café");
        }
    }

    #[test]
    fn handles_empty_input_and_zero_limits() {
        assert_eq!(truncate("", 0), "");
        assert_eq!(truncate("", usize::MAX), "");
        assert_eq!(truncate("🦀", 0), "");
    }

    #[test]
    fn returns_a_slice_of_the_input() {
        let text = "café".to_owned();
        let prefix = truncate(&text, 4);

        assert_eq!(prefix.as_ptr(), text.as_ptr());
    }

    #[test]
    fn appends_the_ellipsis_after_the_byte_limit() {
        assert_eq!(truncate_with_ellipsis("café", 4), "caf…");
    }

    #[test]
    fn appends_a_custom_ellipsis_after_the_byte_limit() {
        assert_eq!(truncate_with("café", 4, "…"), "caf…");
        assert_eq!(truncate_with("café", 4, "..."), "caf...");
        assert_eq!(truncate_with("café", 4, " [more]"), "caf [more]");
    }

    #[test]
    fn borrows_text_when_no_ellipsis_is_needed() {
        for max_len in [5, 6, usize::MAX] {
            let result = truncate_with_ellipsis("café", max_len);

            assert_eq!(result, "café");
            assert!(matches!(result, Cow::Borrowed(_)));
        }
    }

    #[test]
    fn borrows_the_prefix_when_the_ellipsis_is_empty() {
        let result = truncate_with("café", 4, "");

        assert_eq!(result, "caf");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn adds_no_ellipsis_to_empty_input() {
        for max_len in [0, 1, usize::MAX] {
            assert_eq!(truncate_with_ellipsis("", max_len), "");
        }
    }

    #[test]
    fn returns_only_the_ellipsis_when_no_character_fits() {
        assert_eq!(truncate_with_ellipsis("hello", 0), "…");
        assert_eq!(truncate_with_ellipsis("🦀", 3), "…");
    }

    #[test]
    fn ellipsis_does_not_need_to_outlive_the_result() {
        let result = {
            let ellipsis = "…".to_owned();

            truncate_with("café", 4, &ellipsis)
        };

        assert_eq!(result, "caf…");
        assert!(matches!(result, Cow::Owned(_)));
    }
}
