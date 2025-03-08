use std::convert::Infallible;

use actix_utils::future::{Ready, ok};
use actix_web::{FromRequest, HttpRequest, dev::Payload};

/// Host information.
///
/// See [`ConnectionInfo::host()`](actix_web::dev::ConnectionInfo::host) for more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host(String);

impl_more::impl_as_ref!(Host => String);
impl_more::impl_into!(Host => String);
impl_more::forward_display!(Host);

impl Host {
    /// Unwraps into inner string value.
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl FromRequest for Host {
    type Error = Infallible;
    type Future = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ok(Host(req.connection_info().host().to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        App, HttpResponse,
        http::StatusCode,
        test::{self, TestRequest},
        web,
    };

    use super::*;

    #[actix_web::test]
    async fn extracts_host() {
        let app =
            test::init_service(App::new().default_service(web::to(|host: Host| async move {
                HttpResponse::Ok().body(host.to_string())
            })))
            .await;

        let req = TestRequest::default()
            .insert_header(("host", "in-header.com"))
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(test::read_body(res).await, b"in-header.com".as_ref());

        let req = TestRequest::default().uri("http://in-url.com").to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(test::read_body(res).await, b"in-url.com".as_ref());

        let req = TestRequest::default().to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(test::read_body(res).await, b"localhost:8080".as_ref());
    }
}
