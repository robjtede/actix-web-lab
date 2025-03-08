//! For middleware documentation, see [`ErrorHandlers`].

use std::{
    fmt,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, ready},
};

use actix_service::{Service, Transform};
use actix_web::{
    Error, Result,
    body::EitherBody,
    dev::{ServiceRequest, ServiceResponse},
    http::StatusCode,
};
use ahash::AHashMap;
use futures_core::future::LocalBoxFuture;
use pin_project_lite::pin_project;

type ErrorHandlerRes<B> = Result<ServiceResponse<EitherBody<B>>>;
type ErrorHandler<B> = dyn Fn(ServiceResponse<B>) -> LocalBoxFuture<'static, ErrorHandlerRes<B>>;
type Handlers<B> = Rc<AHashMap<StatusCode, Box<ErrorHandler<B>>>>;

/// Middleware for registering custom status code based error handlers.
///
/// Register handlers with the `ErrorHandlers::handler()` method to register a custom error handler
/// for a given status code. Handlers can modify existing responses or create completely new ones.
///
/// # Examples
/// ```
/// use actix_web::{
///     App, HttpResponse, Result,
///     body::EitherBody,
///     dev::ServiceResponse,
///     http::{StatusCode, header},
///     web,
/// };
/// use actix_web_lab::middleware::ErrorHandlers;
///
/// async fn add_error_header<B>(
///     mut res: ServiceResponse<B>,
/// ) -> Result<ServiceResponse<EitherBody<B>>> {
///     res.response_mut().headers_mut().insert(
///         header::CONTENT_TYPE,
///         header::HeaderValue::from_static("Error"),
///     );
///     Ok(res.map_into_left_body())
/// }
///
/// let app = App::new()
///     .wrap(ErrorHandlers::new().handler(StatusCode::INTERNAL_SERVER_ERROR, add_error_header))
///     .service(web::resource("/").route(web::get().to(HttpResponse::InternalServerError)));
/// ```
pub struct ErrorHandlers<B> {
    handlers: Handlers<B>,
}

impl<B> fmt::Debug for ErrorHandlers<B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ErrorHandlers")
            .field(
                "handlers",
                &format_args!("[<{} items>]", self.handlers.len()),
            )
            .finish()
    }
}

impl<B> Default for ErrorHandlers<B> {
    fn default() -> Self {
        ErrorHandlers {
            handlers: Default::default(),
        }
    }
}

impl<B> ErrorHandlers<B> {
    /// Construct new `ErrorHandlers` instance.
    pub fn new() -> Self {
        ErrorHandlers::default()
    }

    /// Register error handler for specified status code.
    pub fn handler<F, Fut>(mut self, status: StatusCode, handler: F) -> Self
    where
        F: Fn(ServiceResponse<B>) -> Fut + 'static,
        Fut: Future<Output = ErrorHandlerRes<B>> + 'static,
    {
        Rc::get_mut(&mut self.handlers)
            .unwrap()
            .insert(status, Box::new(move |res| Box::pin((handler)(res))));
        self
    }
}

impl<S, B> Transform<S, ServiceRequest> for ErrorHandlers<B>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Transform = ErrorHandlersMiddleware<S, B>;
    type InitError = ();
    type Future = LocalBoxFuture<'static, Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        let handlers = self.handlers.clone();
        Box::pin(async move { Ok(ErrorHandlersMiddleware { service, handlers }) })
    }
}

/// Middleware for registering custom status code based error handlers.
///
/// See [`ErrorHandlers`].
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct ErrorHandlersMiddleware<S, B> {
    service: S,
    handlers: Handlers<B>,
}

impl<S, B> Service<ServiceRequest> for ErrorHandlersMiddleware<S, B>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<EitherBody<B>>;
    type Error = Error;
    type Future = ErrorHandlersFuture<S::Future, B>;

    actix_service::forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let handlers = self.handlers.clone();
        let fut = self.service.call(req);
        ErrorHandlersFuture::ServiceFuture { fut, handlers }
    }
}

pin_project! {
    #[project = ErrorHandlersProj]
    pub enum ErrorHandlersFuture<Fut, B>
    where
        Fut: Future,
    {
        ServiceFuture {
            #[pin]
            fut: Fut,
            handlers: Handlers<B>,
        },
        ErrorHandlerFuture {
            fut: LocalBoxFuture<'static, Result<ServiceResponse<EitherBody<B>>, Error>>,
        },
    }
}

impl<Fut, B> Future for ErrorHandlersFuture<Fut, B>
where
    Fut: Future<Output = Result<ServiceResponse<B>, Error>>,
{
    type Output = Result<ServiceResponse<EitherBody<B>>, Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            ErrorHandlersProj::ServiceFuture { fut, handlers } => {
                let res = ready!(fut.poll(cx))?;

                match handlers.get(&res.status()) {
                    Some(handler) => {
                        let fut = handler(res);

                        self.as_mut()
                            .set(ErrorHandlersFuture::ErrorHandlerFuture { fut });

                        self.poll(cx)
                    }

                    None => Poll::Ready(Ok(res.map_into_left_body())),
                }
            }

            ErrorHandlersProj::ErrorHandlerFuture { fut } => fut.as_mut().poll(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_service::IntoService;
    use actix_web::{
        body,
        http::{
            StatusCode,
            header::{CONTENT_TYPE, HeaderValue},
        },
        test::{self, TestRequest},
    };
    use bytes::Bytes;

    use super::*;

    #[actix_web::test]
    async fn add_header_error_handler() {
        #[allow(clippy::unnecessary_wraps)]
        async fn error_handler<B>(
            mut res: ServiceResponse<B>,
        ) -> Result<ServiceResponse<EitherBody<B>>> {
            res.response_mut()
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("0001"));

            Ok(res.map_into_left_body())
        }

        let srv = test::status_service(StatusCode::INTERNAL_SERVER_ERROR);

        let mw = ErrorHandlers::new()
            .handler(StatusCode::INTERNAL_SERVER_ERROR, error_handler)
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let resp = test::call_service(&mw, TestRequest::default().to_srv_request()).await;
        assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "0001");
    }

    #[actix_web::test]
    async fn add_header_error_handler_async() {
        #[allow(clippy::unnecessary_wraps)]
        async fn error_handler<B: 'static>(
            mut res: ServiceResponse<B>,
        ) -> Result<ServiceResponse<EitherBody<B>>> {
            res.response_mut()
                .headers_mut()
                .insert(CONTENT_TYPE, HeaderValue::from_static("0001"));

            Ok(res.map_into_left_body())
        }

        let srv = test::status_service(StatusCode::INTERNAL_SERVER_ERROR);

        let mw = ErrorHandlers::new()
            .handler(StatusCode::INTERNAL_SERVER_ERROR, error_handler)
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let resp = test::call_service(&mw, TestRequest::default().to_srv_request()).await;
        assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "0001");
    }

    #[actix_web::test]
    async fn changes_body_type() {
        #[allow(clippy::unnecessary_wraps)]
        async fn error_handler<B>(
            res: ServiceResponse<B>,
        ) -> Result<ServiceResponse<EitherBody<B>>> {
            let (req, res) = res.into_parts();
            let res = res.set_body(Bytes::from("sorry, that's no bueno"));

            let res = ServiceResponse::new(req, res)
                .map_into_boxed_body()
                .map_into_right_body();

            Ok(res)
        }

        let srv = test::status_service(StatusCode::INTERNAL_SERVER_ERROR);

        let mw = ErrorHandlers::new()
            .handler(StatusCode::INTERNAL_SERVER_ERROR, error_handler)
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let res = test::call_service(&mw, TestRequest::default().to_srv_request()).await;
        assert_eq!(test::read_body(res).await, "sorry, that's no bueno");
    }

    #[actix_web::test]
    async fn error_thrown() {
        #[allow(clippy::unnecessary_wraps)]
        async fn error_handler<B>(
            _res: ServiceResponse<B>,
        ) -> Result<ServiceResponse<EitherBody<B>>> {
            Err(actix_web::error::ErrorInternalServerError(
                "error in error handler",
            ))
        }

        let srv = test::status_service(StatusCode::BAD_REQUEST);

        let mw = ErrorHandlers::new()
            .handler(StatusCode::BAD_REQUEST, error_handler)
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let err = mw
            .call(TestRequest::default().to_srv_request())
            .await
            .unwrap_err();
        let res = err.error_response();

        assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            body::to_bytes(res.into_body()).await.unwrap(),
            "error in error handler"
        );
    }
}
