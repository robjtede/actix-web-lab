// Code mostly copied from `tower`:
// https://github.com/tower-rs/tower/tree/5064987f/tower/src/load_shed

//! Load-shedding middleware.

use std::{
    cell::Cell,
    error::Error as StdError,
    fmt,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_service::{Service, Transform};
use actix_utils::future::{Ready, ok};
use actix_web::ResponseError;
use pin_project_lite::pin_project;

/// A middleware that sheds load when the inner service isn't ready.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct LoadShed;

impl LoadShed {
    /// Creates a new load-shedding middleware.
    pub fn new() -> Self {
        LoadShed
    }
}

impl<S: Service<Req>, Req> Transform<S, Req> for LoadShed {
    type Response = S::Response;
    type Error = Overloaded<S::Error>;
    type Transform = LoadShedService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ok(LoadShedService::new(service))
    }
}

/// A service wrapper that sheds load when the inner service isn't ready.
#[derive(Debug)]
pub struct LoadShedService<S> {
    inner: S,
    is_ready: Cell<bool>,
}

impl<S> LoadShedService<S> {
    /// Wraps a service in [`LoadShedService`] middleware.
    pub(crate) fn new(inner: S) -> Self {
        Self {
            inner,
            is_ready: Cell::new(false),
        }
    }
}

impl<S, Req> Service<Req> for LoadShedService<S>
where
    S: Service<Req>,
{
    type Response = S::Response;
    type Error = Overloaded<S::Error>;
    type Future = LoadShedFuture<S::Future>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // We check for readiness here, so that we can know in `call` if
        // the inner service is overloaded or not.
        let is_ready = match self.inner.poll_ready(cx) {
            Poll::Ready(Err(err)) => return Poll::Ready(Err(Overloaded::Service(err))),
            res => res.is_ready(),
        };

        self.is_ready.set(is_ready);

        // But we always report Ready, so that layers above don't wait until
        // the inner service is ready (the entire point of this layer!)
        Poll::Ready(Ok(()))
    }

    fn call(&self, req: Req) -> Self::Future {
        if self.is_ready.get() {
            // readiness only counts once, you need to check again!
            self.is_ready.set(false);
            LoadShedFuture::called(self.inner.call(req))
        } else {
            LoadShedFuture::overloaded()
        }
    }
}

pin_project! {
    /// Future for [`LoadShedService`].
    pub struct LoadShedFuture<F> {
        #[pin]
        state: LoadShedFutureState<F>,
    }
}

pin_project! {
    #[project = LoadShedFutureStateProj]
    enum LoadShedFutureState<F> {
        Called { #[pin] fut: F },
        Overloaded,
    }
}

impl<F> LoadShedFuture<F> {
    pub(crate) fn called(fut: F) -> Self {
        LoadShedFuture {
            state: LoadShedFutureState::Called { fut },
        }
    }

    pub(crate) fn overloaded() -> Self {
        LoadShedFuture {
            state: LoadShedFutureState::Overloaded,
        }
    }
}

impl<F, T, E> Future for LoadShedFuture<F>
where
    F: Future<Output = Result<T, E>>,
{
    type Output = Result<T, Overloaded<E>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.project().state.project() {
            LoadShedFutureStateProj::Called { fut } => {
                Poll::Ready(ready!(fut.poll(cx)).map_err(Overloaded::Service))
            }
            LoadShedFutureStateProj::Overloaded => Poll::Ready(Err(Overloaded::Overloaded)),
        }
    }
}

impl<F> fmt::Debug for LoadShedFuture<F>
where
    // bounds for future-proofing...
    F: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LoadShedFuture")
    }
}

/// An error returned by [`LoadShed`] service when the inner service is not ready to handle any
/// requests at the time of being called.
#[derive(Debug)]
#[non_exhaustive]
pub enum Overloaded<E> {
    /// Service error.
    Service(E),

    /// Service overloaded.
    Overloaded,
}

impl<E: fmt::Display> fmt::Display for Overloaded<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Overloaded::Service(err) => write!(f, "{err}"),
            Overloaded::Overloaded => f.write_str("service overloaded"),
        }
    }
}

impl<E: StdError + 'static> StdError for Overloaded<E> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Overloaded::Service(err) => Some(err),
            Overloaded::Overloaded => None,
        }
    }
}

impl<E> ResponseError for Overloaded<E>
where
    E: fmt::Debug + fmt::Display,
{
    fn status_code(&self) -> actix_http::StatusCode {
        actix_web::http::StatusCode::SERVICE_UNAVAILABLE
    }
}

#[cfg(test)]
mod tests {
    use actix_web::middleware::{Compat, Logger};

    use super::*;

    #[test]
    fn integration() {
        actix_web::App::new()
            .wrap(Compat::new(LoadShed::new()))
            .wrap(Logger::default());
    }
}
