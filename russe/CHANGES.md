# Changes

## Unreleased

- The `Message::id` field is now an `Option<ByteString>`.
- The `Manager::commit_id()` method now receives an `impl Into<ByteString>`.
- When decoding, split input only on UNIX newlines.
- When decoding, yield errors when input contains invalid UTF-8 instead of panicking.

## 0.1.0

- Initial release.
