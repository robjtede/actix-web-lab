use std::{
    future::{ready, Ready},
    marker::PhantomData,
    rc::Rc,
};

use actix_service::{forward_ready, Service, Transform};
use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    Error, HttpMessage,
};
use bytes::BytesMut;
use digest::{Digest, Output};
use futures_core::future::LocalBoxFuture;
use futures_util::StreamExt;

#[derive(Debug, Clone)]
pub struct BodyHash<D: Digest> {
    hash: Output<D>,
}

impl<D: Digest> BodyHash<D> {
    pub fn as_slice(&self) -> &[u8] {
        self.hash.as_slice()
    }

    pub fn into_inner(self) -> Output<D> {
        self.hash
    }
}

/// Middleware that computes a body hash/digest.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct BodyHasher<D: Digest> {
    _phantom: PhantomData<D>,
}

impl<D: Digest> BodyHasher<D> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<S, D, B> Transform<S, ServiceRequest> for BodyHasher<D>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    D: Digest + 'static,
    B: MessageBody,
{
    type Response = S::Response;
    type Error = S::Error;
    type Transform = BodyHasherService<S, D>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(BodyHasherService {
            service: Rc::new(service),
            _phantom: PhantomData,
        }))
    }
}

/// Middleware service for [`BodyHash`].
pub struct BodyHasherService<S, D> {
    service: Rc<S>,
    _phantom: PhantomData<D>,
}

impl<S, D, B> Service<ServiceRequest> for BodyHasherService<S, D>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    D: Digest + 'static,
    B: MessageBody,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);

        Box::pin(async move {
            let (req, mut pl) = req.into_parts();

            let mut running_hash = D::new();

            let mut buf = BytesMut::new();
            while let Some(chunk) = pl.next().await {
                let chunk = chunk?;
                running_hash.update(&chunk);
                buf.extend_from_slice(&chunk);
            }

            let (_, mut pl) = actix_http::h1::Payload::create(true);
            pl.unread_data(buf.freeze());
            let pl = actix_http::Payload::from(pl);

            req.extensions_mut().insert(BodyHash::<D> {
                hash: running_hash.finalize(),
            });

            let req = ServiceRequest::from_parts(req, pl);
            service.call(req).await
        })
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{test, web, App, HttpRequest, HttpResponse};
    use hex_literal::hex;
    use sha2::Sha256;

    use super::*;

    #[actix_web::test]
    async fn correctly_hashes_payload() {
        let app = test::init_service(App::new().wrap(BodyHasher::<Sha256>::new()).route(
            "/",
            web::get().to(|req: HttpRequest| {
                let ext = req.extensions();
                let hash = ext.get::<BodyHash<Sha256>>().unwrap();
                HttpResponse::Ok().body(hash.as_slice().to_vec())
            }),
        ))
        .await;

        let req = test::TestRequest::default().to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("e3b0c442 98fc1c14 9afbf4c8 996fb924 27ae41e4 649b934c a495991b 7852b855")
                .as_ref()
        );

        let req = test::TestRequest::default().set_payload("abc").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad")
                .as_ref()
        );
    }
}
