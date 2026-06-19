# proxyproto

Parsing and serialization for versions 1 and 2 of the [PROXY protocol].

The crate contains the protocol data types and wire-format implementation without any dependency on Actix. Use [`actix-proxy-protocol`] for Actix server integration. Enable the `tokio` feature for async writer convenience methods.

[proxy protocol]: https://www.haproxy.org/download/1.8/doc/proxy-protocol.txt
[`actix-proxy-protocol`]: https://crates.io/crates/actix-proxy-protocol
