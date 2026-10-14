# actix-local-server

<!-- prettier-ignore-start -->

<!-- cargo-rdme start -->

Run Actix TCP services on the current thread.

`serve()` initializes a `Service<TcpStream>`, then passes each accepted TCP stream to it.
Connections run concurrently as local tasks. The service and its futures can own state that is
not `Send` or `Sync`.

The crate manages TCP acceptance and connection tasks. The caller supplies protocol handling,
such as an `actix_http::HttpService`. Await the server directly in `#[tokio::main]` or
`#[tokio::main(flavor = "local")]`. On a multi-thread runtime, the server creates an internal
`LocalSet`. On a local runtime, it uses the runtime directly. The server does not create threads
or install signal handlers.

## Examples

```rust
use std::{convert::Infallible, io, net::TcpListener};

use actix_http::{HttpService, Request, Response, StatusCode, body::MessageBody};
use actix_service::ServiceFactory as _;

async fn index(_req: Request) -> Result<Response<impl MessageBody>, Infallible> {
    Ok(Response::build(StatusCode::OK).body("Hello!"))
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", 8080))?;
    let addr = listener.local_addr()?;

    actix_local_server::serve(listener, async move {
        HttpService::build()
            .local_addr(addr)
            .h1(index)
            .tcp()
            .new_service(())
            .await
            .map_err(|()| io::Error::other("failed to initialize HTTP service"))
    })
    .await
}
```

<!-- cargo-rdme end -->

<!-- prettier-ignore-end -->
