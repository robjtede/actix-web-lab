use std::{
    future::{Future, Ready, ready},
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

use actix_service::{Service, Transform, forward_ready};
use actix_web::{
    Error,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
};
use futures_core::ready;
use pin_project_lite::pin_project;

/// Creates a middleware from an async function that is used as a mapping function for a
/// [`ServiceResponse`].
///
/// # Examples
/// Adds header:
/// ```
/// # use actix_web_lab::middleware::map_response;
/// use actix_web::{body::MessageBody, dev::ServiceResponse, http::header};
///
/// async fn add_header(
///     mut res: ServiceResponse<impl MessageBody>,
/// ) -> actix_web::Result<ServiceResponse<impl MessageBody>> {
///     res.headers_mut()
///         .insert(header::WARNING, header::HeaderValue::from_static("42"));
///
///     Ok(res)
/// }
/// # actix_web::App::new().wrap(map_response(add_header));
/// ```
///
/// Maps body:
/// ```
/// # use actix_web_lab::middleware::map_response;
/// use actix_web::{body::MessageBody, dev::ServiceResponse};
///
/// async fn mutate_body_type(
///     res: ServiceResponse<impl MessageBody + 'static>,
/// ) -> actix_web::Result<ServiceResponse<impl MessageBody>> {
///     Ok(res.map_into_left_body::<()>())
/// }
/// # actix_web::App::new().wrap(map_response(mutate_body_type));
/// ```
pub fn map_response<F>(mapper_fn: F) -> MapResMiddleware<F> {
    MapResMiddleware {
        mw_fn: Rc::new(mapper_fn),
    }
}

/// Middleware transform for [`map_response`].
#[allow(missing_debug_implementations)]
pub struct MapResMiddleware<F> {
    mw_fn: Rc<F>,
}

impl<S, F, Fut, B, B2> Transform<S, ServiceRequest> for MapResMiddleware<F>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    F: Fn(ServiceResponse<B>) -> Fut,
    Fut: Future<Output = Result<ServiceResponse<B2>, Error>>,
    B2: MessageBody,
{
    type Response = ServiceResponse<B2>;
    type Error = Error;
    type Transform = MapResService<S, F, B>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(MapResService {
            service,
            mw_fn: Rc::clone(&self.mw_fn),
            _phantom: PhantomData,
        }))
    }
}

/// Middleware service for [`from_fn`].
#[allow(missing_debug_implementations)]
pub struct MapResService<S, F, B> {
    service: S,
    mw_fn: Rc<F>,
    _phantom: PhantomData<(B,)>,
}

impl<S, F, Fut, B, B2> Service<ServiceRequest> for MapResService<S, F, B>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    F: Fn(ServiceResponse<B>) -> Fut,
    Fut: Future<Output = Result<ServiceResponse<B2>, Error>>,
    B2: MessageBody,
{
    type Response = ServiceResponse<B2>;
    type Error = Error;
    type Future = MapResFut<S::Future, F, Fut>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let mw_fn = Rc::clone(&self.mw_fn);
        let fut = self.service.call(req);

        MapResFut {
            mw_fn,
            state: MapResFutState::Svc { fut },
        }
    }
}

pin_project! {
    pub struct MapResFut<SvcFut, F, FnFut> {
        mw_fn: Rc<F>,
        #[pin]
        state: MapResFutState<SvcFut, FnFut>,
    }
}

pin_project! {
    #[project = MapResFutStateProj]
    enum MapResFutState<SvcFut, FnFut> {
        Svc { #[pin] fut: SvcFut },
        Fn { #[pin] fut: FnFut },
    }
}

impl<SvcFut, B, F, FnFut, B2> Future for MapResFut<SvcFut, F, FnFut>
where
    SvcFut: Future<Output = Result<ServiceResponse<B>, Error>>,
    F: Fn(ServiceResponse<B>) -> FnFut,
    FnFut: Future<Output = Result<ServiceResponse<B2>, Error>>,
{
    type Output = Result<ServiceResponse<B2>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.as_mut().project();

        match this.state.as_mut().project() {
            MapResFutStateProj::Svc { fut } => {
                let res = ready!(fut.poll(cx))?;

                let fut = (this.mw_fn)(res);
                this.state.set(MapResFutState::Fn { fut });
                self.poll(cx)
            }

            MapResFutStateProj::Fn { fut } => fut.poll(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App, HttpResponse,
        http::header::{self, HeaderValue},
        middleware::{Compat, Logger},
        test, web,
    };

    use super::*;

    async fn noop(
        res: ServiceResponse<impl MessageBody>,
    ) -> Result<ServiceResponse<impl MessageBody>, Error> {
        Ok(res)
    }

    async fn add_header(
        mut res: ServiceResponse<impl MessageBody>,
    ) -> Result<ServiceResponse<impl MessageBody>, Error> {
        res.headers_mut()
            .insert(header::WARNING, HeaderValue::from_static("42"));

        Ok(res)
    }

    async fn mutate_body_type(
        res: ServiceResponse<impl MessageBody + 'static>,
    ) -> Result<ServiceResponse<impl MessageBody>, Error> {
        Ok(res.map_into_left_body::<()>())
    }

    #[actix_web::test]
    async fn compat_compat() {
        let _ = App::new().wrap(Compat::new(map_response(noop)));
        let _ = App::new().wrap(Compat::new(map_response(mutate_body_type)));
    }

    #[actix_web::test]
    async fn feels_good() {
        let app = test::init_service(
            App::new()
                .default_service(web::to(HttpResponse::Ok))
                .wrap(map_response(|res| async move { Ok(res) }))
                .wrap(map_response(noop))
                .wrap(map_response(add_header))
                .wrap(Logger::default())
                .wrap(map_response(mutate_body_type)),
        )
        .await;

        let req = test::TestRequest::default().to_request();
        let res = test::call_service(&app, req).await;
        assert!(res.headers().contains_key(header::WARNING));
    }
}
