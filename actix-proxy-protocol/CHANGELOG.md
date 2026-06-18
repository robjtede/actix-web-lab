# Changes

## Unreleased

- Minimum supported Rust version (MSRV) is now 1.88.
- Add PROXY protocol v1 parsing for IPv4, IPv6, and UNKNOWN headers.
- Add PROXY protocol v2 parsing and encoding for IPv4, IPv6, unspecified, and UNIX address blocks.
- Add `Header::add_crc32c_checksum` for writing PROXY protocol v2 CRC32C checksum TLVs.
- Add typed PROXY protocol v2 TLV helpers for ALPN, authority, CRC32C, NOOP, unique ID, SSL, and NETNS values.
- Add a transparent stream wrapper and Actix acceptor for consuming leading PROXY protocol headers before delegating to the wrapped stream.
- Add a TCP proxy example that can add PROXY v1 or v2 headers and consume incoming PROXY protocol headers.

## 0.1.0

- Initial release.
