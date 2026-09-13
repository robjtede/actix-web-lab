# hot-reload-openssl

Reusable TLS credential rotation for OpenSSL 1.1.1 or newer. The library does not depend on Actix Web or an async runtime.

`build` loads and validates the initial PEM certificate chain and private key before it returns. It fails if either file is missing, cannot be parsed, or does not match. It watches both parent directories with native filesystem notifications, so an atomic rename over either file does not remove the watch. It does not poll.

A dedicated thread owns the watcher and does all file reads, parsing, and key validation. Related events are debounced. A failed attempt retains the last valid pair and is retried a bounded number of times. A later filesystem event starts a new batch after retries are exhausted. An unchanged pair is not republished. Successful validation publishes the entire pair with `arc-swap`.

```rust,no_run
let (config, mut watcher) = hot_reload_openssl::Builder::new("cert.pem", "key.pem").build()?;
// Optional: dispatch reload events on a separate thread.
let observer = watcher.spawn_observer(|event| eprintln!("{event:?}"))?;
// Pass config to HttpServer::bind_openssl and keep watcher until shutdown.
# drop(watcher);
# let _ = observer.join();
# Ok::<(), hot_reload_openssl::Error>(())
```

## Interface and lifetime

`Builder::new` takes only the PEM file paths. `build()` loads the initial pair, starts the filesystem worker, and returns `(tls_configuration, watcher)`:

- Keep `watcher` alive until the server stops. Dropping it stops watching, interrupts pending waits, and joins the worker. Drop can wait for an active file read or validation. A TLS configuration can outlive the watcher and retain its last pair.
- Call `watcher.spawn_observer(callback)` if you want reload notifications on a separate thread. It returns a join handle; after dropping the watcher, join it if you need to wait for buffered events to drain. Callbacks must return for the observer to stop. For manual event handling, use `take_events()` instead. Either method consumes the single event stream. `Reloaded` means publication completed. `ReloadFailed` reports each failed attempt, including its number. `WatchFailed` reports native watcher errors; watch coverage can then be impaired. Monitoring is optional. The stream buffers up to 64 events and drops new events when full, so an absent or slow observer cannot block rotation or cause unbounded memory growth.
- Debounce and retry settings are built in: a 100 ms quiet interval and at most five retries, 200 ms apart. These waits occur only after notifications or failed reloads. Continuous events have a maximum debounce window of one second.

The examples call `build` directly during startup and drop the watcher after the server finishes. File reads and validation run on the dedicated worker; `build` waits for initial validation. If you call `build` or drop the watcher while serving requests, use a blocking task when that wait would delay other async work.

## Defaults and customization

The default builder uses OpenSSL's Mozilla intermediate v5 server preset with a minimum of TLS 1.2. It permits TLS 1.3 and uses the preset's cipher suites and key-exchange settings. It does not require client certificates. No application protocol is assumed, so default ALPN selection is left unset.

Use `configure` to adjust the preset. It receives a mutable `SslAcceptorBuilder`, so you can set protocol versions, ciphers, client authentication, ALPN, and session policy. The worker applies the same callback to the frontend and each replacement context. The callback must apply the same policy each time, and must not install credentials or callbacks that change the selected context.

```rust,no_run
use openssl::ssl::SslVersion;

let (config, watcher) = hot_reload_openssl::Builder::new("cert.pem", "key.pem")
    .configure(|config| {
        config.set_min_proto_version(Some(SslVersion::TLS1_3))?;
        Ok(())
    })
    .build()?;
# Ok::<(), hot_reload_openssl::Error>(())
```

Pass the returned builder to `HttpServer::bind_openssl` or `listen_openssl`. Set custom policy through `configure`, since changing only the returned frontend does not update replacement contexts. If you need ALPN, set it in `configure` too. The crate owns the frontend ClientHello callback; do not replace it. Context selection works with and without SNI. LibreSSL and BoringSSL are not supported.

New full TLS handshakes use the replacement credentials. The server and its listener stay running. Existing connections and resumed sessions are **not revoked**. Manage session caches and tickets separately if your policy requires invalidation. Key matching does **not** check certificate expiry, trust, or hostnames. The caller must validate those properties through its certificate issuance and deployment process. The acceptor serves one identity, rather than selecting identities by SNI.

## Run

From the workspace root, generate a development certificate:

```sh
openssl req -x509 -newkey rsa:2048 -nodes -keyout key.pem -out cert.pem \
  -days 1 -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost'
cargo run -p hot-reload-openssl --example actix_web_openssl
```

In another terminal, use [inspect-cert-chain](https://github.com/robjtede/inspect-cert-chain) to check the served certificate serial number:

```sh
inspect-cert-chain --host localhost --port 8443 | rg -A1 '^Serial Number'
curl --cacert cert.pem https://localhost:8443/
```

Generate and install a replacement while the example stays running:

```sh
openssl req -x509 -newkey rsa:2048 -nodes -keyout key.next -out cert.next \
  -days 1 -subj '/CN=localhost' -addext 'subjectAltName=DNS:localhost'
mv cert.next cert.pem
mv key.next key.pem
```

Run the `inspect-cert-chain` command again to make a new full handshake and confirm that the serial number changed. The example reports errors during an incomplete renewal and reports successful reloads. Protect private-key permissions as part of deployment.

Parent directories must remain present and watchable. Direct replacement of the named files, including replacement of their symlinks, is supported. Changes only to a symlink target outside those directories, replacement of a parent directory, or filesystems that do not deliver native notifications are not supported. There is no polling fallback; monitor watcher errors. Native events can be coalesced or lost by the OS; a reported rescan request triggers a reload of both files.

## Verification

```sh
nix develop -c cargo test -p hot-reload-openssl
nix develop -c just clippy
```

The integration tests start an actual `HttpServer`, inspect the peer certificate through fresh TLS connections, and verify the HTTP response. They cover rotation, invalid pairs, bounded retries, atomic renames, updates in separate directories, watcher shutdown, and retained protocol/ALPN policy for OpenSSL.

Related requests: [actix-net#13](https://github.com/actix/actix-net/issues/13) and [actix-web#754](https://github.com/actix/actix-web/issues/754).
