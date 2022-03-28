use std::io;

use actix_http::{error::PayloadError, BoxedPayloadStream};
use actix_web::dev;
use futures_util::StreamExt as _;
use local_channel::mpsc;
use tracing::trace;

// This is pretty general purpose and should probably live in actix-web-lab.

/// Effectively clones payload.
///
/// The cloned payload:
/// - yields identical chunks;
/// - does not poll ahead of original;
/// - does not poll significantly slower than original;
/// - errors signals are propagated, but details are opaque to the copy.
pub(crate) fn fork_request_payload(orig_payload: &mut dev::Payload) -> dev::Payload {
    const TARGET: &str = concat!(module_path!(), "::fork_request_payload");

    let payload = orig_payload.take();

    let (tx, rx) = mpsc::channel();

    let proxy_stream: BoxedPayloadStream = Box::pin(payload.inspect(move |res| {
        match res {
            Ok(chunk) => {
                trace!(target: TARGET, "yielding {} byte chunk", chunk.len());
                tx.send(Ok(chunk.clone())).unwrap();
            }

            Err(err) => tx
                .send(Err(PayloadError::Io(io::Error::new(
                    io::ErrorKind::Other,
                    format!("error from original stream: {err}"),
                ))))
                .unwrap(),
        }
    }));

    trace!(target: TARGET, "creating proxy payload");
    *orig_payload = dev::Payload::from(proxy_stream);

    dev::Payload::Stream {
        payload: Box::pin(rx),
    }
}
