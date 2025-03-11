use std::{
    future::{Ready, ready},
    rc::Rc,
};

use actix_web::{
    HttpResponse, Responder as _,
    body::EitherBody,
    dev::{Service, ServiceRequest, ServiceResponse, Transform, forward_ready},
    http::header::TryIntoHeaderPair,
    web::Redirect,
};
use futures_core::future::LocalBoxFuture;

use crate::header::StrictTransportSecurity;

/// Middleware to redirect traffic to HTTPS if connection is insecure.
///
/// # HSTS
///
/// [HTTP Strict Transport Security (HSTS)] is configurable. Care should be taken when setting up
/// HSTS for your site; misconfiguration can potentially leave parts of your site in an unusable
/// state. By default it is disabled.
///
/// See [`StrictTransportSecurity`] docs for more info.
///
/// # Examples
///
/// ```
/// # use std::time::Duration;
/// # use actix_web::App;
/// use actix_web_lab::{header::StrictTransportSecurity, middleware::RedirectHttps};
///
/// let mw = RedirectHttps::default();
/// let mw = RedirectHttps::default().to_port(8443);
/// let mw = RedirectHttps::with_hsts(StrictTransportSecurity::default());
/// let mw = RedirectHttps::with_hsts(StrictTransportSecurity::new(Duration::from_secs(60 * 60)));
/// let mw = RedirectHttps::with_hsts(StrictTransportSecurity::recommended());
///
/// App::new().wrap(mw)
/// # ;
/// ```
///
/// [HTTP Strict Transport Security (HSTS)]: https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Strict-Transport-Security
#[derive(Debug, Clone, Default)]
pub struct RedirectHttps {
    hsts: Option<StrictTransportSecurity>,
    port: Option<u16>,
}

impl RedirectHttps {
    /// Construct new HTTP redirect middleware with strict transport security configuration.
    pub fn with_hsts(hsts: StrictTransportSecurity) -> Self {
        Self {
            hsts: Some(hsts),
            ..Self::default()
        }
    }

    /// Sets custom secure redirect port.
    ///
    /// By default, no port is set explicitly so the standard HTTPS port (443) is used.
    pub fn to_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
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
            port: self.port,
        }))
    }
}

/// Middleware service implementation for [`RedirectHttps`].
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct RedirectHttpsMiddleware<S> {
    service: Rc<S>,
    hsts: Option<StrictTransportSecurity>,
    port: Option<u16>,
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
        #![allow(clippy::await_holding_refcell_ref)] // RefCell is dropped before await

        let service = Rc::clone(&self.service);
        let hsts = self.hsts;
        let port = self.port;

        Box::pin(async move {
            let (req, pl) = req.into_parts();
            let conn_info = req.connection_info();

            if conn_info.scheme() != "https" {
                let host = conn_info.host();

                // construct equivalent https path
                let parsed_url = url::Url::parse(&format!("http://{host}"));
                let hostname = match &parsed_url {
                    Ok(url) => url.host_str().unwrap_or(""),
                    Err(_) => host.split_once(':').map_or("", |(host, _port)| host),
                };

                let path = req.uri().path();
                let uri = match port {
                    Some(port) => format!("https://{hostname}:{port}{path}"),
                    None => format!("https://{hostname}{path}"),
                };

                // all connection info is acquired
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
fn apply_hsts<B>(res: &mut HttpResponse<B>, hsts: Option<StrictTransportSecurity>) {
    if let Some(hsts) = hsts {
        let (name, val) = hsts.try_into_pair().unwrap();
        res.headers_mut().insert(name, val);
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App, Error, HttpResponse,
        body::MessageBody,
        dev::ServiceFactory,
        http::{
            StatusCode,
            header::{self, Header as _},
        },
        test, web,
    };

    use super::*;
    use crate::{assert_response_matches, test_request};

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
        assert!(res.headers().get(header::LOCATION).is_none());

        let body = test::read_body(res).await;
        assert_eq!(body, "content");
    }

    #[actix_web::test]
    async fn with_hsts() {
        // no HSTS
        let app = RedirectHttps::default()
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = test_request!(GET "http://localhost/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert!(!res.headers().contains_key(StrictTransportSecurity::name()));

        let req = test_request!(GET "https://localhost:443/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert!(!res.headers().contains_key(StrictTransportSecurity::name()));

        // with HSTS
        let app = RedirectHttps::with_hsts(StrictTransportSecurity::recommended())
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = test_request!(GET "http://localhost/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert!(res.headers().contains_key(StrictTransportSecurity::name()));

        let req = test_request!(GET "https://localhost:443/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert!(res.headers().contains_key(StrictTransportSecurity::name()));
    }

    #[actix_web::test]
    async fn to_custom_port() {
        let app = RedirectHttps::default()
            .to_port(8443)
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = test_request!(GET "http://localhost/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert_response_matches!(res, TEMPORARY_REDIRECT; "location" => "https://localhost:8443/");
    }

    #[actix_web::test]
    async fn to_ipv6() {
        let app = RedirectHttps::default()
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = test_request!(GET "http://[fe80::1234:1234:1234:1234]/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert_response_matches!(res, TEMPORARY_REDIRECT; "location" => "https://[fe80::1234:1234:1234:1234]/");
    }

    #[actix_web::test]
    async fn to_custom_port_when_port_in_host() {
        let app = RedirectHttps::default()
            .to_port(8443)
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = test_request!(GET "http://localhost:8080/").to_srv_request();
        let res = test::call_service(&app, req).await;
        assert_response_matches!(res, TEMPORARY_REDIRECT; "location" => "https://localhost:8443/");
    }
}
