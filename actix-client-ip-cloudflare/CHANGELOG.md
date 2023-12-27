# Changelog

## Unreleased

- Replace visible types from `cidr-utils` with equivalent types from the `ipnetwork` crate.

## 0.1.1

- Add `TrustedIps::new()` constructor.
- Add `TrustedIps::add_ip_range()` method.
- Add `TrustedIps::{add_loopback_ips, add_private_ips}()` methods.
- Implement `Default` for `TrustedIps`.
- Add `CfConnectingIp[v6]::is_trusted()` method.
- Deprecate `TrustedIps::with_ip_range()` method.

## 0.1.0

- Initial release.
