# Changes

## Unreleased

## 0.2.0

- Change the `Acceptor` service factory's associated future type to `std::future::Ready`.

## 0.1.0

- Add a transparent stream wrapper and Actix acceptor for consuming leading PROXY protocol headers before delegating to the wrapped stream.
- Minimum supported Rust version (MSRV) is now 1.88.

## 0.0.2

- Initial release.
