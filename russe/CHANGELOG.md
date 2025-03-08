# Changes

## Unreleased

- Upgrade to edition 2024.
- Minimum supported Rust version (MSRV) is now 1.85.

## 0.0.5

- The `Message::id` field is now an `Option<ByteString>`.
- The `Manager::commit_id()` method now receives an `impl Into<ByteString>`.
- When decoding, split input only on UNIX newlines.
- When decoding, yield errors when input contains invalid UTF-8 instead of panicking.

## 0.0.4

- Initial release.
