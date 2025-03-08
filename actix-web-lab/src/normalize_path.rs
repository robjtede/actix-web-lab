//! For middleware documentation, see [`NormalizePath`].

use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_service::{Service, Transform};
use actix_utils::future::{Ready, ready};
use actix_web::{
    Error, HttpResponse,
    body::EitherBody,
    dev::{ServiceRequest, ServiceResponse},
    http::{
        StatusCode, header,
        uri::{PathAndQuery, Uri},
    },
    middleware::TrailingSlash,
};
use bytes::Bytes;
use pin_project_lite::pin_project;
use regex::Regex;

/// Middleware for normalizing a request's path so that routes can be matched more flexibly.
///
/// # Normalization Steps
/// - Merges consecutive slashes into one. (For example, `/path//one` always becomes `/path/one`.)
/// - Appends a trailing slash if one is not present, removes one if present, or keeps trailing
///   slashes as-is, depending on which [`TrailingSlash`] variant is supplied
///   to [`new`](NormalizePath::new()).
///
/// # Default Behavior
/// The default constructor chooses to strip trailing slashes from the end of paths with them
/// ([`TrailingSlash::Trim`]). The implication is that route definitions should be defined without
/// trailing slashes or else they will be inaccessible (or vice versa when using the
/// `TrailingSlash::Always` behavior), as shown in the example tests below.
///
/// # Examples
/// ```
/// use actix_web::{App, middleware, web};
///
/// # actix_web::rt::System::new().block_on(async {
/// let app = App::new()
///     .wrap(middleware::NormalizePath::trim())
///     .route("/test", web::get().to(|| async { "test" }))
///     .route("/unmatchable/", web::get().to(|| async { "unmatchable" }));
///
/// use actix_web::{
///     http::StatusCode,
///     test::{TestRequest, call_service, init_service},
/// };
///
/// let app = init_service(app).await;
///
/// let req = TestRequest::with_uri("/test").to_request();
/// let res = call_service(&app, req).await;
/// assert_eq!(res.status(), StatusCode::OK);
///
/// let req = TestRequest::with_uri("/test/").to_request();
/// let res = call_service(&app, req).await;
/// assert_eq!(res.status(), StatusCode::OK);
///
/// let req = TestRequest::with_uri("/unmatchable").to_request();
/// let res = call_service(&app, req).await;
/// assert_eq!(res.status(), StatusCode::NOT_FOUND);
///
/// let req = TestRequest::with_uri("/unmatchable/").to_request();
/// let res = call_service(&app, req).await;
/// assert_eq!(res.status(), StatusCode::NOT_FOUND);
/// # })
/// ```
#[derive(Debug, Clone, Copy)]
pub struct NormalizePath {
    /// Controls path normalization behavior.
    trailing_slash_behavior: TrailingSlash,

    /// Returns redirects for non-normalized paths if `Some`.
    use_redirects: Option<StatusCode>,
}

impl Default for NormalizePath {
    fn default() -> Self {
        Self {
            trailing_slash_behavior: TrailingSlash::Trim,
            use_redirects: None,
        }
    }
}

impl NormalizePath {
    /// Create new `NormalizePath` middleware with the specified trailing slash style.
    pub fn new(behavior: TrailingSlash) -> Self {
        Self {
            trailing_slash_behavior: behavior,
            use_redirects: None,
        }
    }

    /// Constructs a new `NormalizePath` middleware with [trim](TrailingSlash::Trim) semantics.
    ///
    /// Use this instead of `NormalizePath::default()` to avoid deprecation warning.
    pub fn trim() -> Self {
        Self::new(TrailingSlash::Trim)
    }

    /// Configures middleware to respond to requests with non-normalized paths with a 307 redirect.
    ///
    /// If configured
    ///
    /// For example, a request with the path `/api//v1/foo/` would receive a response with a
    /// `Location: /api/v1/foo` header (assuming `Trim` trailing slash behavior.)
    ///
    /// To customize the status code, use [`use_redirects_with`](Self::use_redirects_with).
    pub fn use_redirects(mut self) -> Self {
        self.use_redirects = Some(StatusCode::TEMPORARY_REDIRECT);
        self
    }

    /// Configures middleware to respond to requests with non-normalized paths with a redirect.
    ///
    /// For example, a request with the path `/api//v1/foo/` would receive a 307 response with a
    /// `Location: /api/v1/foo` header (assuming `Trim` trailing slash behavior.)
    ///
    /// # Panics
    /// Panics if `status_code` is not a redirect (300-399).
    pub fn use_redirects_with(mut self, status_code: StatusCode) -> Self {
        assert!(status_code.is_redirection());
        self.use_redirects = Some(status_code);
        self
    }
}

impl<S, B> Transform<S, ServiceRequest> for NormalizePath
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<EitherBody<B, ()>>;
    type Error = Error;
    type Transform = NormalizePathService<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(NormalizePathService {
            service,
            merge_slash: Regex::new("//+").unwrap(),
            trailing_slash_behavior: self.trailing_slash_behavior,
            use_redirects: self.use_redirects,
        }))
    }
}

/// Middleware service implementation for [`NormalizePath`].
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct NormalizePathService<S> {
    service: S,
    merge_slash: Regex,
    trailing_slash_behavior: TrailingSlash,
    use_redirects: Option<StatusCode>,
}

impl<S, B> Service<ServiceRequest> for NormalizePathService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
{
    type Response = ServiceResponse<EitherBody<B, ()>>;
    type Error = Error;
    type Future = NormalizePathFuture<S, B>;

    actix_service::forward_ready!(service);

    fn call(&self, mut req: ServiceRequest) -> Self::Future {
        let head = req.head_mut();

        let mut path_altered = false;
        let original_path = head.uri.path();

        // An empty path here means that the URI has no valid path. We skip normalization in this
        // case, because adding a path can make the URI invalid
        if !original_path.is_empty() {
            // Either adds a string to the end (duplicates will be removed anyways) or trims all
            // slashes from the end
            let path = match self.trailing_slash_behavior {
                TrailingSlash::Always => format!("{original_path}/"),
                TrailingSlash::MergeOnly => original_path.to_string(),
                TrailingSlash::Trim => original_path.trim_end_matches('/').to_string(),
                ts_behavior => panic!("unknown trailing slash behavior: {ts_behavior:?}"),
            };

            // normalize multiple /'s to one /
            let path = self.merge_slash.replace_all(&path, "/");

            // Ensure root paths are still resolvable. If resulting path is blank after previous
            // step it means the path was one or more slashes. Reduce to single slash.
            let path = if path.is_empty() { "/" } else { path.as_ref() };

            // Check whether the path has been changed
            //
            // This check was previously implemented as string length comparison
            //
            // That approach fails when a trailing slash is added,
            // and a duplicate slash is removed,
            // since the length of the strings remains the same
            //
            // For example, the path "/v1//s" will be normalized to "/v1/s/"
            // Both of the paths have the same length,
            // so the change can not be deduced from the length comparison
            if path != original_path {
                let mut parts = head.uri.clone().into_parts();
                let query = parts.path_and_query.as_ref().and_then(|pq| pq.query());

                let path = match query {
                    Some(query) => Bytes::from(format!("{path}?{query}")),
                    None => Bytes::copy_from_slice(path.as_bytes()),
                };
                parts.path_and_query = Some(PathAndQuery::from_maybe_shared(path).unwrap());

                let uri = Uri::from_parts(parts).unwrap();
                req.match_info_mut().get_mut().update(&uri);
                req.head_mut().uri = uri;

                path_altered = true;
            }
        }

        match self.use_redirects {
            Some(code) if path_altered => {
                let mut res = HttpResponse::with_body(code, ());
                res.headers_mut().insert(
                    header::LOCATION,
                    req.head_mut().uri.to_string().parse().unwrap(),
                );
                NormalizePathFuture::redirect(req.into_response(res))
            }

            _ => NormalizePathFuture::service(self.service.call(req)),
        }
    }
}

pin_project! {
    pub struct NormalizePathFuture<S: Service<ServiceRequest>, B> {
        #[pin] inner: Inner<S, B>,
    }
}

impl<S: Service<ServiceRequest>, B> NormalizePathFuture<S, B> {
    fn service(fut: S::Future) -> Self {
        Self {
            inner: Inner::Service {
                fut,
                _body: PhantomData,
            },
        }
    }

    fn redirect(res: ServiceResponse<()>) -> Self {
        Self {
            inner: Inner::Redirect { res: Some(res) },
        }
    }
}

pin_project! {
    #[project = InnerProj]
    enum Inner<S: Service<ServiceRequest>, B> {
        Redirect { res: Option<ServiceResponse<()>>, },
        Service {
            #[pin] fut: S::Future,
            _body: PhantomData<B>,
        },
    }
}

impl<S, B> Future for NormalizePathFuture<S, B>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
{
    type Output = Result<ServiceResponse<EitherBody<B, ()>>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        match this.inner.project() {
            InnerProj::Redirect { res } => {
                Poll::Ready(Ok(res.take().unwrap().map_into_right_body()))
            }

            InnerProj::Service { fut, .. } => {
                let res = ready!(fut.poll(cx))?;
                Poll::Ready(Ok(res.map_into_left_body()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_service::IntoService;
    use actix_web::{
        App, HttpRequest, HttpResponse,
        dev::ServiceRequest,
        guard::fn_guard,
        test::{self, TestRequest, call_service, init_service},
        web,
    };

    use super::*;

    #[actix_web::test]
    async fn default_is_trim_no_redirect() {
        let app = init_service(App::new().wrap(NormalizePath::default()).service(
            web::resource("/test").to(|req: HttpRequest| async move { req.path().to_owned() }),
        ))
        .await;

        let req = TestRequest::with_uri("/test/").to_request();
        let res = call_service(&app, req).await;
        assert!(res.status().is_success());
        assert_eq!(test::read_body(res).await, "/test");
    }

    #[actix_web::test]
    async fn trim_trailing_slashes() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::trim())
                .service(web::resource("/").to(HttpResponse::Ok))
                .service(web::resource("/v1/something").to(HttpResponse::Ok))
                .service(
                    web::resource("/v2/something")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        let test_uris = vec![
            "/",
            "/?query=test",
            "///",
            "/v1//something",
            "/v1//something////",
            "//v1/something",
            "//v1//////something",
            "/v2//something?query=test",
            "/v2//something////?query=test",
            "//v2/something?query=test",
            "//v2//////something?query=test",
        ];

        for uri in test_uris {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;
            assert!(res.status().is_success(), "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn always_trailing_slashes() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::new(TrailingSlash::Always))
                .service(web::resource("/").to(HttpResponse::Ok))
                .service(web::resource("/v1/something/").to(HttpResponse::Ok))
                .service(
                    web::resource("/v2/something/")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        let test_uris = vec![
            "/",
            "///",
            "/v1/something",
            "/v1/something/",
            "/v1/something////",
            "//v1//something",
            "//v1//something//",
            "/v2/something?query=test",
            "/v2/something/?query=test",
            "/v2/something////?query=test",
            "//v2//something?query=test",
            "//v2//something//?query=test",
        ];

        for uri in test_uris {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;
            assert!(res.status().is_success(), "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn trim_root_trailing_slashes_with_query() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::new(TrailingSlash::Trim))
                .service(
                    web::resource("/")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        let test_uris = vec!["/?query=test", "//?query=test", "///?query=test"];

        for uri in test_uris {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;
            assert!(res.status().is_success(), "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn ensure_trailing_slash() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::new(TrailingSlash::Always))
                .service(web::resource("/").to(HttpResponse::Ok))
                .service(web::resource("/v1/something/").to(HttpResponse::Ok))
                .service(
                    web::resource("/v2/something/")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        let test_uris = vec![
            "/",
            "///",
            "/v1/something",
            "/v1/something/",
            "/v1/something////",
            "//v1//something",
            "//v1//something//",
            "/v2/something?query=test",
            "/v2/something/?query=test",
            "/v2/something////?query=test",
            "//v2//something?query=test",
            "//v2//something//?query=test",
        ];

        for uri in test_uris {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;
            assert!(res.status().is_success(), "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn ensure_root_trailing_slash_with_query() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::new(TrailingSlash::Always))
                .service(
                    web::resource("/")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        let test_uris = vec!["/?query=test", "//?query=test", "///?query=test"];

        for uri in test_uris {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;
            assert!(res.status().is_success(), "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn keep_trailing_slash_unchanged() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::new(TrailingSlash::MergeOnly))
                .service(web::resource("/").to(HttpResponse::Ok))
                .service(web::resource("/v1/something").to(HttpResponse::Ok))
                .service(web::resource("/v1/").to(HttpResponse::Ok))
                .service(
                    web::resource("/v2/something")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        let tests = vec![
            ("/", true), // root paths should still work
            ("/?query=test", true),
            ("///", true),
            ("/v1/something////", false),
            ("/v1/something/", false),
            ("//v1//something", true),
            ("/v1/", true),
            ("/v1", false),
            ("/v1////", true),
            ("//v1//", true),
            ("///v1", false),
            ("/v2/something?query=test", true),
            ("/v2/something/?query=test", false),
            ("/v2/something//?query=test", false),
            ("//v2//something?query=test", true),
        ];

        for (uri, success) in tests {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;
            assert_eq!(res.status().is_success(), success, "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn no_path() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::default())
                .service(web::resource("/").to(HttpResponse::Ok)),
        )
        .await;

        // This URI will be interpreted as an authority form, i.e. there is no path nor scheme
        // (https://datatracker.ietf.org/doc/html/rfc7230#section-5.3.3)
        let req = TestRequest::with_uri("eh").to_request();
        let res = call_service(&app, req).await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
    }

    #[actix_web::test]
    async fn test_in_place_normalization() {
        let srv = |req: ServiceRequest| {
            assert_eq!("/v1/something", req.path());
            ready(Ok(req.into_response(HttpResponse::Ok().finish())))
        };

        let normalize = NormalizePath::default()
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let test_uris = vec![
            "/v1//something////",
            "///v1/something",
            "//v1///something",
            "/v1//something",
        ];

        for uri in test_uris {
            let req = TestRequest::with_uri(uri).to_srv_request();
            let res = normalize.call(req).await.unwrap();
            assert!(res.status().is_success(), "Failed uri: {uri}");
        }
    }

    #[actix_web::test]
    async fn should_normalize_nothing() {
        const URI: &str = "/v1/something";

        let srv = |req: ServiceRequest| {
            assert_eq!(URI, req.path());
            ready(Ok(req.into_response(HttpResponse::Ok().finish())))
        };

        let normalize = NormalizePath::default()
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let req = TestRequest::with_uri(URI).to_srv_request();
        let res = normalize.call(req).await.unwrap();
        assert!(res.status().is_success());
    }

    #[actix_web::test]
    async fn should_normalize_no_trail() {
        let srv = |req: ServiceRequest| {
            assert_eq!("/v1/something", req.path());
            ready(Ok(req.into_response(HttpResponse::Ok().finish())))
        };

        let normalize = NormalizePath::default()
            .new_transform(srv.into_service())
            .await
            .unwrap();

        let req = TestRequest::with_uri("/v1/something/").to_srv_request();
        let res = normalize.call(req).await.unwrap();
        assert!(res.status().is_success());
    }

    #[actix_web::test]
    async fn should_return_redirects_when_configured() {
        let normalize = NormalizePath::trim()
            .use_redirects()
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = TestRequest::with_uri("/v1/something/").to_srv_request();
        let res = normalize.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);

        let normalize = NormalizePath::trim()
            .use_redirects_with(StatusCode::PERMANENT_REDIRECT)
            .new_transform(test::ok_service())
            .await
            .unwrap();

        let req = TestRequest::with_uri("/v1/something/").to_srv_request();
        let res = normalize.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT);
    }

    #[actix_web::test]
    async fn trim_with_redirect() {
        let app = init_service(
            App::new()
                .wrap(NormalizePath::trim().use_redirects())
                .service(web::resource("/").to(HttpResponse::Ok))
                .service(web::resource("/v1/something").to(HttpResponse::Ok))
                .service(
                    web::resource("/v2/something")
                        .guard(fn_guard(|ctx| ctx.head().uri.query() == Some("query=test")))
                        .to(HttpResponse::Ok),
                ),
        )
        .await;

        // list of uri and if it should result in a redirect
        let test_uris = vec![
            ("/", false),
            ("///", true),
            ("/v1/something", false),
            ("/v1/something/", true),
            ("/v1/something////", true),
            ("//v1//something", true),
            ("//v1//something//", true),
            ("/v2/something?query=test", false),
            ("/v2/something/?query=test", true),
            ("/v2/something////?query=test", true),
            ("//v2//something?query=test", true),
            ("//v2//something//?query=test", true),
        ];

        for (uri, should_redirect) in test_uris {
            let req = TestRequest::with_uri(uri).to_request();
            let res = call_service(&app, req).await;

            if should_redirect {
                assert!(res.status().is_redirection(), "URI did not redirect: {uri}");
            } else {
                assert!(res.status().is_success(), "Failed URI: {uri}");
            }
        }
    }
}
