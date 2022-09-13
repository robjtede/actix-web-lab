use actix_web::{
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    Error, Responder,
};

use crate::{middleware_from_fn::Next, web::Redirect};

/// A function middleware to redirect traffic to `www.` if not already there.
///
/// # Examples
/// ```
/// # use actix_web::App;
/// use actix_web_lab::middleware::{from_fn, redirect_to_www};
///
/// App::new()
///     .wrap(from_fn(redirect_to_www))
///     # ;
/// ```
pub async fn redirect_to_www(
    req: ServiceRequest,
    next: Next<impl MessageBody + 'static>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    #![allow(clippy::await_holding_refcell_ref)] // RefCell is dropped before await

    let (req, pl) = req.into_parts();
    let conn_info = req.connection_info();

    if !conn_info.host().starts_with("www.") {
        let scheme = conn_info.scheme();
        let host = conn_info.host();
        let path = req.uri().path();
        let uri = format!("{scheme}://www.{host}{path}");

        let res = Redirect::to(uri).respond_to(&req);

        drop(conn_info);
        return Ok(ServiceResponse::new(req, res).map_into_right_body());
    }

    drop(conn_info);
    let req = ServiceRequest::from_parts(req, pl);
    Ok(next.call(req).await?.map_into_left_body())
}

#[cfg(test)]
mod test_super {
    use actix_web::{
        dev::ServiceFactory,
        http::{header, StatusCode},
        test, web, App, HttpResponse,
    };

    use super::*;
    use crate::middleware::from_fn;

    fn test_app() -> App<
        impl ServiceFactory<
            ServiceRequest,
            Response = ServiceResponse<impl MessageBody>,
            Config = (),
            InitError = (),
            Error = Error,
        >,
    > {
        App::new().wrap(from_fn(redirect_to_www)).route(
            "/",
            web::get().to(|| async { HttpResponse::Ok().body("content") }),
        )
    }

    #[actix_web::test]
    async fn redirect_non_www() {
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::default().to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

        let loc = res.headers().get(header::LOCATION);
        assert!(loc.is_some());
        assert!(loc.unwrap().as_bytes().starts_with(b"http://www"));

        let body = test::read_body(res).await;
        assert!(body.is_empty());
    }

    #[actix_web::test]
    async fn do_not_redirect_already_www() {
        let app = test::init_service(test_app()).await;

        let req = test::TestRequest::default()
            .uri("http://www.localhost/")
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK);

        let loc = res.headers().get(header::LOCATION);
        assert!(loc.is_none());

        let body = test::read_body(res).await;
        assert_eq!(body, "content");
    }
}
