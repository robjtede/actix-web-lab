use std::{
    future::{ready, Ready},
    rc::Rc,
};

use actix_web::{
    body::EitherBody,
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    http::header::TryIntoHeaderPair,
    HttpResponse, Responder as _,
};
use futures_core::future::LocalBoxFuture;

use crate::{header::Hsts, web::Redirect};

/// A middleware to redirect traffic to HTTPS if connection is insecure.
///
/// # HSTS
/// [HTTP Strict Transport Security (HSTS)] is configurable. Care should be taken when setting up
/// HSTS for your site; misconfiguration can potentially leave parts of your site in an unusable
/// state. By default it is disabled.
///
/// See [`Hsts`] docs for more info.
///
/// # Examples
/// ```rust
/// # use std::time::Duration;
/// # use actix_web::App;
/// use actix_web_lab::{header::Hsts, middleware::RedirectHttps};
///
/// App::new().wrap(RedirectHttps::default());
/// App::new().wrap(RedirectHttps::with_hsts(Hsts::default()));
/// App::new().wrap(RedirectHttps::with_hsts(Hsts::new(Duration::from_secs(60 * 60))));
/// App::new().wrap(RedirectHttps::with_hsts(Hsts::recommended()));
/// ```
///
/// [HTTP Strict Transport Security (HSTS)]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Strict-Transport-Security
#[derive(Debug, Clone, Default)]
pub struct RedirectHttps {
    hsts: Option<Hsts>,
}

impl RedirectHttps {
    /// Construct new HTTP redirect middleware with
    pub fn with_hsts(hsts: Hsts) -> Self {
        Self { hsts: Some(hsts) }
    }
}

impl<S, B> Transform<S, ServiceRequest> for RedirectHttps
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>> + 'static,
{
    type Response = ServiceResponse<EitherBody<B, ()>>;
    type Error = S::Error;
    type Transform = RedirectHttpsMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RedirectHttpsMiddleware {
            service: Rc::new(service),
            hsts: self.hsts,
        }))
    }
}

pub struct RedirectHttpsMiddleware<S> {
    service: Rc<S>,
    hsts: Option<Hsts>,
}

impl<S, B> Service<ServiceRequest> for RedirectHttpsMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>> + 'static,
{
    type Response = ServiceResponse<EitherBody<B, ()>>;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let service = Rc::clone(&self.service);
        let hsts = self.hsts;

        Box::pin(async move {
            let (req, pl) = req.into_parts();
            let conn_info = req.connection_info();

            if conn_info.scheme() != "https" {
                // construct equivalent https path
                let host = conn_info.host();
                let path = req.uri().path();
                let uri = format!("https://{host}{path}");
                drop(conn_info);

                // create redirection response
                let redirect = Redirect::to(uri);

                let mut res = redirect.respond_to(&req).map_into_right_body();
                apply_hsts(&mut res, hsts);

                return Ok(ServiceResponse::new(req, res));
            }

            drop(conn_info);

            let req = ServiceRequest::from_parts(req, pl);

            // TODO: apply HSTS header to error case

            service.call(req).await.map(|mut res| {
                apply_hsts(res.response_mut(), hsts);
                res.map_into_left_body()
            })
        })
    }
}

/// Apply HSTS config to an `HttpResponse`.
fn apply_hsts<B>(res: &mut HttpResponse<B>, hsts: Option<Hsts>) {
    if let Some(hsts) = hsts {
        let (name, val) = hsts.try_into_pair().unwrap();
        res.headers_mut().insert(name, val);
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        body::MessageBody,
        dev::ServiceFactory,
        http::{
            header::{self, Header as _},
            StatusCode,
        },
        test, web, App, Error, HttpResponse,
    };

    use super::*;

    fn test_app() -> App<
        impl ServiceFactory<
            ServiceRequest,
            Response = ServiceResponse<impl MessageBody>,
            Config = (),
            InitError = (),
            Error = Error,
        >,
    > {
        App::new().wrap(RedirectHttps::default()).route(
            "/",
            web::get().to(|| async { HttpResponse::Ok().body("content") }),
        )
    }

    #[actix_web::test]
    async fn redirect_non_https() {
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::default().to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

        let loc = res.headers().get(header::LOCATION);
        assert!(loc.is_some());
        assert!(loc.unwrap().as_bytes().starts_with(b"https://"));

        let body = test::read_body(res).await;
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn do_not_redirect_already_https() {
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::default()
            .uri("https://localhost:443/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK);

        let loc = res.headers().get(header::LOCATION);
        assert!(loc.is_none());

        let body = test::read_body(res).await;
        assert_eq!(body, "content");
    }

    #[actix_web::test]
    async fn with_hsts() {
        // no HSTS
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::default()
            .uri("http://localhost:443/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert!(!res.headers().contains_key(Hsts::name()));

        let req = test::TestRequest::default()
            .uri("https://localhost:443/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert!(!res.headers().contains_key(Hsts::name()));

        // with HSTS
        let app = test::init_service(
            App::new()
                .wrap(RedirectHttps::with_hsts(Hsts::recommended()))
                .route(
                    "/",
                    web::get().to(|| async { HttpResponse::Ok().body("content") }),
                ),
        )
        .await;

        let req = test::TestRequest::default()
            .uri("http://localhost:443/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert!(res.headers().contains_key(Hsts::name()));

        let req = test::TestRequest::default()
            .uri("https://localhost:443/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert!(res.headers().contains_key(Hsts::name()));
    }
}
