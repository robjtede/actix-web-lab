use actix_http::BoxedPayloadStream;
use actix_web::{dev, FromRequest, HttpRequest};
use bytes::Bytes;
use futures_core::future::LocalBoxFuture;
use futures_util::StreamExt as _;
use local_channel::mpsc;
use tokio::try_join;

pub(crate) fn body_fold<T, Init, Out>(
    req: &HttpRequest,
    payload: &mut dev::Payload,
    init: Init,
    mut update_fn: impl FnMut(&mut Init, &HttpRequest, Bytes) + 'static,
    mut finalize_fn: impl FnMut(T, Init) -> Out + 'static,
) -> LocalBoxFuture<'static, Result<Out, T::Error>>
where
    T: FromRequest,
    Init: 'static,
{
    let req = req.clone();
    let payload = payload.take();

    Box::pin(async move {
        let (tx, mut rx) = mpsc::channel();

        // wrap payload in stream that reads chunks and clones them (cheaply) back here
        let proxy_stream: BoxedPayloadStream = Box::pin(payload.inspect(move |res| {
            if let Ok(chunk) = res {
                log::trace!("yielding {} byte chunk", chunk.len());
                tx.send(chunk.clone()).unwrap();
            }
        }));

        log::trace!("creating proxy payload");
        let mut proxy_payload = dev::Payload::from(proxy_stream);
        let body_fut = T::from_request(&req, &mut proxy_payload);

        // compute body hash as chunks are yielded from channel
        let hash_fut = async {
            let mut accumulator = init;
            while let Some(chunk) = rx.recv().await {
                log::trace!("updating hasher with {} byte chunk", chunk.len());
                update_fn(&mut accumulator, &req, chunk)
            }
            Ok(accumulator)
        };

        log::trace!("driving both futures");
        let (body, hash) = try_join!(body_fut, hash_fut)?;

        let out = (finalize_fn)(body, hash);

        Ok(out)
    })
}
