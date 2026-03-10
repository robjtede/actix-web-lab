use std::{
    future::{Ready, ready},
    rc::Rc,
};

use actix_web::{
    Responder as _,
    body::EitherBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    web::Redirect,
};
use futures_core::future::LocalBoxFuture;

use crate::redirect_host::{HostAllowlist, reject_untrusted_host};

/// Middleware to redirect traffic away from `www.` if it's present.
///
/// # Security
///
/// This middleware constructs absolute redirect URLs from request-derived connection information.
/// If your deployment accepts unvalidated `Host` or forwarding headers, an attacker can influence
/// the redirect target.
///
/// To harden this middleware, configure [`RedirectToNonWww::allow_hosts`]. Requests with
/// non-allowlisted hosts receive a `400 Bad Request` response instead of a redirect. Without an
/// allowlist, you should validate hosts upstream before requests reach the application.
///
/// # Examples
///
/// ```
/// # use actix_web::App;
/// use actix_web_lab::middleware::RedirectToNonWww;
///
/// let mw = RedirectToNonWww::default();
/// let mw = RedirectToNonWww::default().allow_hosts(["example.com", "www.example.com"]);
///
/// App::new().wrap(mw)
/// # ;
/// ```
#[derive(Debug, Clone, Default)]
pub struct RedirectToNonWww {
    allowed_hosts: Option<HostAllowlist>,
}

impl RedirectToNonWww {
    /// Restricts redirect behavior to requests whose host matches an allowlist entry.
    ///
    /// Requests with non-allowlisted hosts receive a `400 Bad Request` response instead of a
    /// redirect.
    pub fn allow_hosts<I, S>(mut self, hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.allowed_hosts = Some(HostAllowlist::new(hosts));
        self
    }
}

impl<S, B> Transform<S, ServiceRequest> for RedirectToNonWww
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>> + 'static,
{
    type Response = ServiceResponse<EitherBody<B, ()>>;
    type Error = S::Error;
    type Transform = RedirectToNonWwwMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(RedirectToNonWwwMiddleware {
            service: Rc::new(service),
            allowed_hosts: self.allowed_hosts.clone(),
        }))
    }
}

/// Middleware service implementation for [`RedirectToNonWww`].
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct RedirectToNonWwwMiddleware<S> {
    service: Rc<S>,
    allowed_hosts: Option<HostAllowlist>,
}

impl<S, B> Service<ServiceRequest> for RedirectToNonWwwMiddleware<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>> + 'static,
{
    type Response = ServiceResponse<EitherBody<B, ()>>;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        #![allow(clippy::await_holding_refcell_ref)] // RefCell is dropped before await

        let service = Rc::clone(&self.service);
        let allowed_hosts = self.allowed_hosts.clone();

        Box::pin(async move {
            let (req, pl) = req.into_parts();
            let conn_info = req.connection_info();
            let host = conn_info.host();

            if let Some(res) = reject_untrusted_host(allowed_hosts.as_ref(), host) {
                drop(conn_info);
                return Ok(ServiceResponse::new(req, res.map_into_right_body()));
            }

            if let Some(host_no_www) = host.strip_prefix("www.") {
                let scheme = if conn_info.scheme() == "https" {
                    "https"
                } else {
                    "http"
                };
                let path = req.uri().path();
                let uri = format!("{scheme}://{host_no_www}{path}");

                let res = Redirect::to(uri).respond_to(&req).map_into_right_body();

                drop(conn_info);
                return Ok(ServiceResponse::new(req, res));
            }

            drop(conn_info);
            let req = ServiceRequest::from_parts(req, pl);
            service.call(req).await.map(|res| res.map_into_left_body())
        })
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App, Error, HttpResponse,
        body::MessageBody,
        dev::ServiceFactory,
        http::{StatusCode, header},
        test, web,
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
        App::new().wrap(RedirectToNonWww::default()).route(
            "/",
            web::get().to(|| async { HttpResponse::Ok().body("content") }),
        )
    }

    #[actix_web::test]
    async fn redirect_non_www() {
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::get()
            .uri("http://www.localhost/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

        let loc = res.headers().get(header::LOCATION);
        assert!(loc.is_some());
        assert!(!loc.unwrap().as_bytes().starts_with(b"http://www."));

        let body = test::read_body(res).await;
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn do_not_redirect_already_non_www() {
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::default()
            .uri("http://localhost/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK);

        let loc = res.headers().get(header::LOCATION);
        assert!(loc.is_none());

        let body = test::read_body(res).await;
        assert_eq!(body, "content");
    }

    #[actix_web::test]
    async fn reject_unallowlisted_host() {
        let app = test::init_service(
            App::new()
                .wrap(RedirectToNonWww::default().allow_hosts(["example.com", "www.example.com"]))
                .route(
                    "/",
                    web::get().to(|| async { HttpResponse::Ok().body("content") }),
                ),
        )
        .await;

        let req = test::TestRequest::default()
            .insert_header(("host", "www.attacker.example"))
            .to_request();
        let res = test::call_service(&app, req).await;

        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        assert!(res.headers().get(header::LOCATION).is_none());
    }

    #[actix_web::test]
    async fn redirect_allowlisted_host() {
        let app = test::init_service(
            App::new()
                .wrap(RedirectToNonWww::default().allow_hosts(["example.com", "www.example.com"]))
                .route(
                    "/",
                    web::get().to(|| async { HttpResponse::Ok().body("content") }),
                ),
        )
        .await;

        let req = test::TestRequest::default()
            .insert_header(("host", "www.example.com"))
            .to_request();
        let res = test::call_service(&app, req).await;

        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            res.headers().get(header::LOCATION).unwrap(),
            "http://example.com/"
        );
    }
}
