# Changes

## Unreleased

- Preserve leading empty lines in decoded message data.
- Avoid decoder panics for lines with no recognized directive.
- Preserve all lines in decoded multiline comments.

## 0.0.8

- Add `ReqwestExt` and `Manager` APIs for Reqwest 0.13 through the optional `reqwest-0_13` crate feature.

## 0.0.7

- Add `Encoder` for SSE messages, comments, and retry delays using Tokio's `Encoder<Event>` trait.

## 0.0.6

- Add `reqwest-0_13` crate feature (off-by-default).
- Upgrade to edition 2024.
- Minimum supported Rust version (MSRV) is now 1.88.

## 0.0.5

- The `Message::id` field is now an `Option<ByteString>`.
- The `Manager::commit_id()` method now receives an `impl Into<ByteString>`.
- When decoding, split input only on UNIX newlines.
- When decoding, yield errors when input contains invalid UTF-8 instead of panicking.

## 0.0.4

- Initial release.
