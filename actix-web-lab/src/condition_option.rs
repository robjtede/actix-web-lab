//! For middleware documentation, see [`ConditionOption`].

use std::{
    pin::Pin,
    task::{self, Poll, ready},
};

use actix_web::{
    body::EitherBody,
    dev::{Service, ServiceResponse, Transform},
};
use futures_core::future::LocalBoxFuture;
use futures_util::future::FutureExt as _;
use pin_project_lite::pin_project;

/// Middleware for conditionally enabling other middleware in an [`Option`].
///
/// # Example
/// ```
/// use actix_web::{App, middleware::Logger};
/// use actix_web_lab::middleware::ConditionOption;
///
/// let normalize: ConditionOption<_> = Some(Logger::default()).into();
/// let app = App::new().wrap(normalize);
/// ```
#[derive(Debug)]
pub struct ConditionOption<T> {
    inner: Option<T>,
}

impl<T> From<Option<T>> for ConditionOption<T> {
    fn from(value: Option<T>) -> Self {
        Self { inner: value }
    }
}

impl<S, T, Req, BE, BD, Err> Transform<S, Req> for ConditionOption<T>
where
    S: Service<Req, Response = ServiceResponse<BD>, Error = Err> + 'static,
    T: Transform<S, Req, Response = ServiceResponse<BE>, Error = Err>,
    T::Future: 'static,
    T::InitError: 'static,
    T::Transform: 'static,
{
    type Response = ServiceResponse<EitherBody<BE, BD>>;
    type Error = Err;
    type Transform = ConditionMiddleware<T::Transform, S>;
    type InitError = T::InitError;
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        match &self.inner {
            Some(transformer) => {
                let fut = transformer.new_transform(service);
                async move {
                    let wrapped_svc = fut.await?;
                    Ok(ConditionMiddleware::Enable(wrapped_svc))
                }
                .boxed_local()
            }
            None => async move { Ok(ConditionMiddleware::Disable(service)) }.boxed_local(),
        }
    }
}

/// TODO
#[derive(Debug)]
pub enum ConditionMiddleware<E, D> {
    Enable(E),
    Disable(D),
}

impl<E, D, Req, BE, BD, Err> Service<Req> for ConditionMiddleware<E, D>
where
    E: Service<Req, Response = ServiceResponse<BE>, Error = Err>,
    D: Service<Req, Response = ServiceResponse<BD>, Error = Err>,
{
    type Response = ServiceResponse<EitherBody<BE, BD>>;
    type Error = Err;
    type Future = ConditionMiddlewareFuture<E::Future, D::Future>;

    fn poll_ready(&self, cx: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self {
            ConditionMiddleware::Enable(service) => service.poll_ready(cx),
            ConditionMiddleware::Disable(service) => service.poll_ready(cx),
        }
    }

    fn call(&self, req: Req) -> Self::Future {
        match self {
            ConditionMiddleware::Enable(service) => ConditionMiddlewareFuture::Enabled {
                fut: service.call(req),
            },
            ConditionMiddleware::Disable(service) => ConditionMiddlewareFuture::Disabled {
                fut: service.call(req),
            },
        }
    }
}

pin_project! {
    #[doc(hidden)]
    #[project = ConditionProj]
    pub enum ConditionMiddlewareFuture<E, D> {
        Enabled { #[pin] fut: E, },
        Disabled { #[pin] fut: D, },
    }
}

impl<E, D, BE, BD, Err> Future for ConditionMiddlewareFuture<E, D>
where
    E: Future<Output = Result<ServiceResponse<BE>, Err>>,
    D: Future<Output = Result<ServiceResponse<BD>, Err>>,
{
    type Output = Result<ServiceResponse<EitherBody<BE, BD>>, Err>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        let res = match self.project() {
            ConditionProj::Enabled { fut } => ready!(fut.poll(cx))?.map_into_left_body(),
            ConditionProj::Disabled { fut } => ready!(fut.poll(cx))?.map_into_right_body(),
        };

        Poll::Ready(Ok(res))
    }
}
