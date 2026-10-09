//! Tests for byte limits, UTF-8 boundaries, and optional ellipses.

use std::borrow::Cow;

use truncate_safe::{truncate, truncate_with, truncate_with_ellipsis};

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
    let text = String::from("café");
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
        let ellipsis = String::from("…");

        truncate_with("café", 4, &ellipsis)
    };

    assert_eq!(result, "caf…");
    assert!(matches!(result, Cow::Owned(_)));
}
