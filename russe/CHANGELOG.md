# Changes

## Unreleased

## 0.1.0

- Preserve leading empty lines in decoded message data.
- Avoid decoder panics for lines with no recognized directive.
- Preserve all lines in decoded multiline comments.
- Preserve message data and retry directives in frames that also contain comments.
- Decode event streams with CRLF line endings.
- Decode bare CR and mixed line endings, including delimiters split across reads.
- Ignore unknown fields and empty frames while continuing to decode buffered events.
- Decode fields without a colon as fields with an empty value.
- Ignore invalid retry values without discarding a previous valid retry value.
- Decode retry delays larger than u64 milliseconds when Duration can represent them.

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
