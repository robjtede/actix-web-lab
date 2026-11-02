use std::{fmt, rc::Rc};

use actix_service::{Service, ServiceExt as _, ServiceFactory, Transform, boxed};
use actix_web::{
    Error, Resource, Route, Scope,
    body::MessageBody,
    dev::{ServiceRequest, ServiceResponse},
    web::ServiceConfig,
};
use futures_util::future::LocalBoxFuture;

type HttpService = boxed::BoxService<ServiceRequest, ServiceResponse, Error>;
type Wrap = Rc<dyn Fn(HttpService) -> LocalBoxFuture<'static, Result<HttpService, ()>>>;
type Register = Box<dyn FnOnce(&mut ServiceConfig, GroupMiddleware)>;

/// A collection of routes and services with common middleware and no path prefix.
///
/// Register with `App::configure(group.into_config())` or
/// `Scope::configure(group.into_config())`. Children register directly in the parent,
/// in declaration order. Unmatched requests can reach siblings and the parent default.
/// A child scope retains its prefix matching and default service.
///
/// Supports routes, resources, scopes, nested groups, vectors, arrays, and tuples of
/// up to twelve supported services. Opaque services, including macro-generated handlers, do
/// not implement [`GroupService`]. Custom adapters can implement that trait.
///
/// Middleware applies to every child, regardless of builder call order. The last
/// middleware added runs first on requests. Factories need not implement `Clone`;
/// each child gets a separate middleware service instance. Bodies are boxed between
/// middleware layers. Use shared state explicitly when children must share state.
///
/// Unlike the proposed core Actix Web group, middleware is installed through each
/// child's public `wrap` method. It sees child resource or scope app data and runs
/// outside middleware already installed on that child. A scope's middleware can
/// rewrite the URI before that scope selects a route. Groups have no app data,
/// guards, or default service of their own.
///
/// # Examples
/// ```
/// use actix_web::{App, HttpResponse, middleware::DefaultHeaders, web};
/// use actix_web_lab::web::group;
///
/// let app = App::new()
///     .configure(
///         group((
///             web::resource("/dashboard").to(HttpResponse::Ok),
///             web::scope("/users").route("/{id}", web::get().to(HttpResponse::Ok)),
///         ))
///         .wrap(DefaultHeaders::new().add(("x-group", "private")))
///         .into_config(),
///     )
///     .route("/public", web::get().to(HttpResponse::Ok));
/// ```
///
/// Children with the same type can use an array:
/// ```
/// use actix_web::{App, HttpResponse, middleware::DefaultHeaders, web};
/// use actix_web_lab::web::group;
///
/// let app = App::new().configure(
///     group([
///         web::resource("/a").to(HttpResponse::Ok),
///         web::resource("/b").to(HttpResponse::Accepted),
///     ])
///     .wrap(DefaultHeaders::new().add(("x-group", "shared")))
///     .into_config(),
/// );
/// ```
#[derive(Default)]
pub struct Group {
    services: Vec<Register>,
    middleware: GroupMiddleware,
}

impl fmt::Debug for Group {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Group")
            .field("children", &self.services.len())
            .finish_non_exhaustive()
    }
}

impl Group {
    /// Creates an empty group.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a supported child or collection of children.
    pub fn service<S: GroupService>(mut self, service: S) -> Self {
        self.services.push(Box::new(move |config, middleware| {
            service.register_group(config, middleware);
        }));

        self
    }

    /// Registers a route, preserving its guards for parent routing.
    ///
    /// Complete the route's handler configuration before passing it here.
    pub fn route(mut self, path: &str, route: Route) -> Self {
        let path = path.to_owned();
        self.services.push(Box::new(move |config, middleware| {
            config.route(&path, route.wrap(middleware));
        }));

        self
    }

    /// Applies middleware to every child, including children already added.
    pub fn wrap<M, B>(mut self, middleware: M) -> Self
    where
        M: Transform<
                HttpService,
                ServiceRequest,
                Response = ServiceResponse<B>,
                Error = Error,
                InitError = (),
            > + 'static,
        B: MessageBody + 'static,
    {
        let previous = self.middleware;
        let middleware = Rc::new(middleware);

        self.middleware = GroupMiddleware(Rc::new(move |service| {
            let previous = previous.clone();
            let middleware = Rc::clone(&middleware);

            Box::pin(async move {
                let service = (previous.0)(service).await?;
                let service = middleware.new_transform(service).await?;

                Ok(box_service(service))
            })
        }));

        self
    }

    /// Converts this group into a callback for `App::configure` or `Scope::configure`.
    pub fn into_config(self) -> impl FnOnce(&mut ServiceConfig) {
        move |config| self.register_group(config, GroupMiddleware::default())
    }
}

/// A service that can register itself with group middleware.
///
/// Implementations must apply `middleware` to each child before registering it
/// directly in `config`. Do not introduce an empty scope: it would claim requests
/// intended for siblings. An arbitrary `HttpServiceFactory` cannot be adapted
/// unless its API provides a way to install middleware.
pub trait GroupService: 'static {
    /// Applies group middleware and registers the service in its parent.
    fn register_group(self, config: &mut ServiceConfig, middleware: GroupMiddleware);
}

/// Shared middleware factories for a group's children.
///
/// Custom [`GroupService`] adapters can pass this value to a service's `wrap` method.
/// Clones share factories; each `new_transform` call creates fresh service instances.
#[derive(Clone)]
pub struct GroupMiddleware(Wrap);

impl fmt::Debug for GroupMiddleware {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupMiddleware").finish_non_exhaustive()
    }
}

impl Default for GroupMiddleware {
    fn default() -> Self {
        Self(Rc::new(|service| Box::pin(async { Ok(service) })))
    }
}

fn box_service<S, B>(service: S) -> HttpService
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    boxed::service(service.map(|response| response.map_into_boxed_body()))
}

impl<S, B> Transform<S, ServiceRequest> for GroupMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error> + 'static,
    S::Future: 'static,
    B: MessageBody + 'static,
{
    type Response = ServiceResponse;
    type Error = Error;
    type InitError = ();
    type Transform = HttpService;
    type Future = LocalBoxFuture<'static, Result<HttpService, ()>>;

    fn new_transform(&self, service: S) -> Self::Future {
        (self.0)(box_service(service))
    }
}

impl GroupService for Group {
    fn register_group(self, config: &mut ServiceConfig, middleware: GroupMiddleware) {
        let inner = self.middleware;
        let combined = GroupMiddleware(Rc::new(move |service| {
            let inner = inner.clone();
            let outer = middleware.clone();

            Box::pin(async move {
                let service = (inner.0)(service).await?;
                (outer.0)(service).await
            })
        }));

        for register in self.services {
            register(config, combined.clone());
        }
    }
}

macro_rules! impl_group_service {
    ($ty:ident) => {
        impl<T, B> GroupService for $ty<T>
        where
            T: ServiceFactory<
                    ServiceRequest,
                    Config = (),
                    Response = ServiceResponse<B>,
                    Error = Error,
                    InitError = (),
                > + 'static,
            T::Future: 'static,
            T::Service: 'static,
            <T::Service as Service<ServiceRequest>>::Future: 'static,
            B: MessageBody + 'static,
        {
            fn register_group(self, config: &mut ServiceConfig, middleware: GroupMiddleware) {
                config.service(self.wrap(middleware));
            }
        }
    };
}

impl_group_service!(Resource);
impl_group_service!(Scope);

impl<S: GroupService> GroupService for Vec<S> {
    fn register_group(self, config: &mut ServiceConfig, middleware: GroupMiddleware) {
        for service in self {
            service.register_group(config, middleware.clone());
        }
    }
}

impl<S: GroupService, const N: usize> GroupService for [S; N] {
    fn register_group(self, config: &mut ServiceConfig, middleware: GroupMiddleware) {
        for service in self {
            service.register_group(config, middleware.clone());
        }
    }
}

macro_rules! impl_group_tuple {
    ($($ty:ident),+) => {
        impl<$($ty: GroupService),+> GroupService for ($($ty,)+) {
            #[allow(non_snake_case)]
            fn register_group(self, config: &mut ServiceConfig, middleware: GroupMiddleware) {
                let ($($ty,)+) = self;
                $($ty.register_group(config, middleware.clone());)+
            }
        }
    };
}

impl_group_tuple!(A);
impl_group_tuple!(A, B);
impl_group_tuple!(A, B, C);
impl_group_tuple!(A, B, C, D);
impl_group_tuple!(A, B, C, D, E);
impl_group_tuple!(A, B, C, D, E, F);
impl_group_tuple!(A, B, C, D, E, F, G);
impl_group_tuple!(A, B, C, D, E, F, G, H);
impl_group_tuple!(A, B, C, D, E, F, G, H, I);
impl_group_tuple!(A, B, C, D, E, F, G, H, I, J);
impl_group_tuple!(A, B, C, D, E, F, G, H, I, J, K);
impl_group_tuple!(A, B, C, D, E, F, G, H, I, J, K, L);

#[cfg(test)]
mod tests {
    use actix_web::{
        App, HttpResponse,
        body::{BoxBody, MessageBody},
        dev::{ServiceRequest, ServiceResponse},
        http::{Method, StatusCode},
        middleware::{DefaultHeaders, Next, from_fn},
        test, web,
    };

    use crate::web::{Group, group};

    #[actix_web::test]
    async fn groups_preserve_siblings_guards_and_parent_default() {
        let app = test::init_service(
            App::new()
                .configure(
                    Group::new()
                        .route("/item", web::get().to(|| async { "get" }))
                        .wrap(DefaultHeaders::new().add(("x-group", "yes")))
                        .into_config(),
                )
                .configure(
                    Group::new()
                        .route("/item", web::post().to(|| async { "post" }))
                        .into_config(),
                )
                .route("/public", web::get().to(|| async { "public" }))
                .default_service(web::to(|| async {
                    HttpResponse::NotFound().body("default")
                })),
        )
        .await;

        for (method, path, expected, grouped) in [
            (Method::GET, "/item", "get", true),
            (Method::POST, "/item", "post", false),
            (Method::DELETE, "/item", "default", false),
            (Method::GET, "/public", "public", false),
            (Method::GET, "/unknown", "default", false),
        ] {
            let res = test::call_service(
                &app,
                test::TestRequest::with_uri(path)
                    .method(method)
                    .to_request(),
            )
            .await;
            assert_eq!(res.headers().contains_key("x-group"), grouped);
            assert_eq!(test::read_body(res).await, expected);
        }
    }

    #[actix_web::test]
    async fn collections_and_nested_groups_preserve_names_paths_and_defaults() {
        let app = test::init_service(
            App::new().service(
                web::scope("/api").configure(
                    group((
                        group(vec![web::resource("/item/{id}").name("item").to(
                            |req: actix_web::HttpRequest| async move {
                                format!(
                                    "{}|{}",
                                    req.match_pattern().unwrap(),
                                    req.url_for("item", ["8"]).unwrap()
                                )
                            },
                        )]),
                        web::scope("/child").default_service(web::to(|| async {
                            HttpResponse::NotFound().body("child")
                        })),
                        web::resource("/method").route(web::get().to(HttpResponse::Ok)),
                    ))
                    .wrap(DefaultHeaders::new().add(("x-group", "yes")))
                    .into_config(),
                ),
            ),
        )
        .await;

        let res = test::call_service(
            &app,
            test::TestRequest::with_uri("/api/item/7")
                .insert_header(("host", "example.com"))
                .to_request(),
        )
        .await;
        assert_eq!(res.headers().get("x-group").unwrap(), "yes");
        assert_eq!(
            test::read_body(res).await,
            "/api/item/{id}|http://example.com/api/item/8"
        );

        let res = test::call_service(
            &app,
            test::TestRequest::with_uri("/api/child/missing").to_request(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.headers().get("x-group").unwrap(), "yes");
        assert_eq!(test::read_body(res).await, "child");

        let res = test::call_service(
            &app,
            test::TestRequest::post().uri("/api/method").to_request(),
        )
        .await;
        assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(res.headers().get("x-group").unwrap(), "yes");
    }

    async fn stamp(
        name: &'static str,
        mut req: ServiceRequest,
        next: Next<BoxBody>,
    ) -> Result<ServiceResponse<BoxBody>, actix_web::Error> {
        use actix_web::http::header::{HeaderName, HeaderValue};
        req.headers_mut().append(
            HeaderName::from_static("x-order"),
            HeaderValue::from_static(name),
        );
        let mut res = next.call(req).await?;
        res.headers_mut().append(
            HeaderName::from_static("x-order"),
            HeaderValue::from_static(name),
        );
        Ok(res)
    }

    #[actix_web::test]
    async fn nested_groups_preserve_middleware_order() {
        let app = test::init_service(
            App::new().configure(
                group(
                    Group::new()
                        .wrap(from_fn(|req, next| stamp("inner", req, next)))
                        .route(
                            "/order",
                            web::get().to(|req: actix_web::HttpRequest| async move {
                                req.headers()
                                    .get_all("x-order")
                                    .map(|v| v.to_str().unwrap())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            }),
                        ),
                )
                .wrap(from_fn(|req, next| stamp("middle", req, next)))
                .wrap(from_fn(|req, next| stamp("outer", req, next)))
                .into_config(),
            ),
        )
        .await;

        let res =
            test::call_service(&app, test::TestRequest::with_uri("/order").to_request()).await;
        assert_eq!(
            res.headers()
                .get_all("x-order")
                .map(|v| v.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["inner", "middle", "outer"]
        );
        assert_eq!(test::read_body(res).await, "outer,middle,inner");
    }

    #[actix_web::test]
    async fn middleware_sees_child_data() {
        async fn report(
            data: web::Data<u32>,
            req: ServiceRequest,
            next: Next<BoxBody>,
        ) -> Result<ServiceResponse<BoxBody>, actix_web::Error> {
            let res = next.call(req).await?;
            Ok(res.map_body(|_, _| BoxBody::new(data.to_string())))
        }

        let app = test::init_service(
            App::new().app_data(web::Data::new(1u32)).configure(
                Group::new()
                    .wrap(from_fn(report))
                    .service((
                        web::scope("/scope")
                            .app_data(web::Data::new(2u32))
                            .route("", web::get().to(HttpResponse::Ok)),
                        web::resource("/resource")
                            .app_data(web::Data::new(3u32))
                            .to(HttpResponse::Ok),
                    ))
                    .into_config(),
            ),
        )
        .await;

        for (path, expected) in [("/scope", "2"), ("/resource", "3")] {
            assert_eq!(
                test::call_and_read_body(&app, test::TestRequest::with_uri(path).to_request())
                    .await,
                expected
            );
        }
    }

    #[actix_web::test]
    async fn concrete_bodies_are_boxed_between_layers() {
        async fn string(
            req: ServiceRequest,
            next: Next<BoxBody>,
        ) -> Result<ServiceResponse<String>, actix_web::Error> {
            Ok(next.call(req).await?.map_body(|_, _| "body".to_owned()))
        }
        async fn boxed(
            req: ServiceRequest,
            next: Next<BoxBody>,
        ) -> Result<ServiceResponse<impl MessageBody>, actix_web::Error> {
            next.call(req).await
        }

        let app = test::init_service(
            App::new().configure(
                Group::new()
                    .wrap(from_fn(string))
                    .wrap(from_fn(boxed))
                    .route("/body", web::get().to(HttpResponse::Accepted))
                    .into_config(),
            ),
        )
        .await;
        let res = test::call_service(&app, test::TestRequest::with_uri("/body").to_request()).await;
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        assert_eq!(test::read_body(res).await, "body");
    }

    struct VisitCounter;

    impl<S> actix_web::dev::Transform<S, ServiceRequest> for VisitCounter
    where
        S: actix_web::dev::Service<
                ServiceRequest,
                Response = ServiceResponse,
                Error = actix_web::Error,
            > + 'static,
        S::Future: 'static,
    {
        type Response = ServiceResponse;
        type Error = actix_web::Error;
        type InitError = ();
        type Transform =
            actix_service::boxed::BoxService<ServiceRequest, Self::Response, Self::Error>;
        type Future = std::future::Ready<Result<Self::Transform, ()>>;

        fn new_transform(&self, service: S) -> Self::Future {
            let visits = std::cell::Cell::new(0);

            std::future::ready(Ok(actix_service::boxed::service(actix_service::apply_fn(
                service,
                move |mut req: ServiceRequest, service: &S| {
                    use actix_web::http::header::{HeaderName, HeaderValue};
                    visits.set(visits.get() + 1);
                    req.headers_mut().insert(
                        HeaderName::from_static("x-visits"),
                        HeaderValue::from_str(&visits.get().to_string()).unwrap(),
                    );
                    service.call(req)
                },
            ))))
        }
    }

    #[actix_web::test]
    async fn non_clone_middleware_has_separate_state_per_child() {
        async fn visits(req: actix_web::HttpRequest) -> String {
            req.headers()
                .get("x-visits")
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned()
        }

        let app = test::init_service(
            App::new().configure(
                Group::new()
                    .wrap(VisitCounter)
                    .route("/a", web::get().to(visits))
                    .service(web::resource("/b").to(visits))
                    .service(
                        web::scope("/scope")
                            .route("/a", web::get().to(visits))
                            .route("/b", web::get().to(visits)),
                    )
                    .into_config(),
            ),
        )
        .await;

        for (path, expected) in [
            ("/a", "1"),
            ("/a", "2"),
            ("/b", "1"),
            ("/scope/a", "1"),
            ("/scope/b", "2"),
            ("/b", "2"),
        ] {
            assert_eq!(
                test::call_and_read_body(&app, test::TestRequest::with_uri(path).to_request())
                    .await,
                expected
            );
        }
    }

    #[actix_web::test]
    async fn uri_rewrites_stay_in_selected_child_but_can_select_scope_routes() {
        async fn rewrite(
            mut req: ServiceRequest,
            next: Next<BoxBody>,
        ) -> Result<ServiceResponse, actix_web::Error> {
            let uri = "/target".parse::<actix_web::http::Uri>().unwrap();
            req.match_info_mut()
                .set(actix_web::dev::Url::new(uri.clone()));
            req.head_mut().uri = uri;
            next.call(req).await
        }

        let app = test::init_service(
            App::new().configure(
                Group::new()
                    .wrap(from_fn(rewrite))
                    .route("/original", web::get().to(|| async { "original" }))
                    .route("/target", web::get().to(|| async { "target" }))
                    .service(
                        web::scope("/scope")
                            .route("/original", web::get().to(|| async { "scope original" }))
                            .route("/target", web::get().to(|| async { "scope target" })),
                    )
                    .into_config(),
            ),
        )
        .await;

        for (path, expected) in [
            ("/original", "original"),
            ("/scope/original", "scope target"),
        ] {
            assert_eq!(
                test::call_and_read_body(&app, test::TestRequest::with_uri(path).to_request())
                    .await,
                expected
            );
        }
    }

    #[actix_web::test]
    async fn group_middleware_runs_outside_existing_child_middleware() {
        let app = test::init_service(
            App::new().configure(
                Group::new()
                    .wrap(from_fn(|req, next| stamp("group", req, next)))
                    .service(
                        web::resource("/order")
                            .wrap(from_fn(|req, next| stamp("child", req, next)))
                            .to(|req: actix_web::HttpRequest| async move {
                                req.headers()
                                    .get_all("x-order")
                                    .map(|v| v.to_str().unwrap())
                                    .collect::<Vec<_>>()
                                    .join(",")
                            }),
                    )
                    .into_config(),
            ),
        )
        .await;

        assert_eq!(
            test::call_and_read_body(&app, test::TestRequest::with_uri("/order").to_request())
                .await,
            "group,child"
        );
    }

    #[actix_web::test]
    async fn empty_group_does_not_claim_requests() {
        let app = test::init_service(
            App::new()
                .configure(Group::new().wrap(VisitCounter).into_config())
                .route("/public", web::get().to(|| async { "public" })),
        )
        .await;

        assert_eq!(
            test::call_and_read_body(&app, test::TestRequest::with_uri("/public").to_request())
                .await,
            "public"
        );
    }

    #[actix_web::test]
    async fn custom_adapters_and_arrays_receive_group_middleware() {
        struct Endpoint(&'static str);

        impl crate::web::GroupService for Endpoint {
            fn register_group(
                self,
                config: &mut web::ServiceConfig,
                middleware: crate::web::GroupMiddleware,
            ) {
                config.service(web::resource(self.0).wrap(middleware).to(HttpResponse::Ok));
            }
        }

        let app = test::init_service(
            App::new().configure(
                group([Endpoint("/a"), Endpoint("/b")])
                    .wrap(DefaultHeaders::new().add(("x-group", "yes")))
                    .into_config(),
            ),
        )
        .await;

        for path in ["/a", "/b"] {
            let res =
                test::call_service(&app, test::TestRequest::with_uri(path).to_request()).await;
            assert_eq!(res.status(), StatusCode::OK);
            assert_eq!(res.headers().get("x-group").unwrap(), "yes");
        }
    }

    #[actix_web::test]
    async fn constructor_children_and_later_additions_share_middleware() {
        let app = test::init_service(
            App::new().configure(
                group([
                    web::resource("/a").to(HttpResponse::Ok),
                    web::resource("/b").to(HttpResponse::Accepted),
                ])
                .wrap(DefaultHeaders::new().add(("x-group", "yes")))
                .route("/c", web::get().to(HttpResponse::Created))
                .service(web::resource("/d").to(HttpResponse::NoContent))
                .into_config(),
            ),
        )
        .await;

        for (path, status) in [
            ("/a", StatusCode::OK),
            ("/b", StatusCode::ACCEPTED),
            ("/c", StatusCode::CREATED),
            ("/d", StatusCode::NO_CONTENT),
        ] {
            let res =
                test::call_service(&app, test::TestRequest::with_uri(path).to_request()).await;
            assert_eq!(res.status(), status);
            assert_eq!(res.headers().get("x-group").unwrap(), "yes");
        }
    }
}
