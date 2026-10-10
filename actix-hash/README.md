# actix-hash

<!-- prettier-ignore-start -->

[![crates.io](https://img.shields.io/crates/v/actix-hash?label=latest)](https://crates.io/crates/actix-hash)
[![Documentation](https://docs.rs/actix-hash/badge.svg?version=0.5.1)](https://docs.rs/actix-hash/0.5.1)
![MIT or Apache 2.0 licensed](https://img.shields.io/crates/l/actix-hash.svg)
<br />
[![dependency status](https://deps.rs/crate/actix-hash/0.5.1/status.svg)](https://deps.rs/crate/actix-hash/0.5.1)
[![Download](https://img.shields.io/crates/d/actix-hash.svg)](https://crates.io/crates/actix-hash)

<!-- cargo-rdme start -->

Hashing utilities for Actix Web.

## Crate Features
All features are enabled by default.
- `blake2`: Blake2 types
- `blake3`: Blake3 types
- `md5`: MD5 types 🚩
- `md4`: MD4 types 🚩
- `sha1`: SHA-1 types 🚩
- `sha2`: SHA-2 types
- `sha3`: SHA-3 types

## Security Warning 🚩
The `md4`, `md5`, and `sha1` types are included for completeness and interoperability but they
are considered cryptographically broken by modern standards. For security critical use cases,
you should move to using the other algorithms.

<!-- cargo-rdme end -->

<!-- prettier-ignore-end -->
