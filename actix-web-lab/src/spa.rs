use std::borrow::Cow;

use actix_files::{Files, NamedFile};
use actix_service::fn_service;
use actix_web::dev::{HttpServiceFactory, ResourceDef, ServiceRequest, ServiceResponse};
use tracing::trace;

/// Single Page App (SPA) service builder.
///
/// # Examples
///
/// ```no_run
/// # use actix_web::App;
/// # use actix_web_lab::web::spa;
/// App::new()
///     // ...api routes...
///     .service(
///         spa()
///             .index_file("./examples/assets/spa.html")
///             .static_resources_mount("/static")
///             .static_resources_location("./examples/assets")
///             .finish(),
///     )
/// # ;
/// ```
#[derive(Debug, Clone)]
pub struct Spa {
    index_file: Cow<'static, str>,
    static_resources_mount: Cow<'static, str>,
    static_resources_location: Cow<'static, str>,
}

impl Spa {
    /// Location of the SPA index file.
    ///
    /// This file will be served if:
    /// - the Actix Web router has reached this service, indicating that none of the API routes
    ///   matched the URL path;
    /// - and none of the static resources handled matched.
    ///
    /// The default is "./index.html". I.e., the `index.html` file located in the directory that
    /// the server is running from.
    pub fn index_file(mut self, index_file: impl Into<Cow<'static, str>>) -> Self {
        self.index_file = index_file.into();
        self
    }

    /// The URL path prefix that static files should be served from.
    ///
    /// The default is "/". I.e., static files are served from the root URL path.
    pub fn static_resources_mount(
        mut self,
        static_resources_mount: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.static_resources_mount = static_resources_mount.into();
        self
    }

    /// The location in the filesystem to serve static resources from.
    ///
    /// The default is "./". I.e., static files are located in the directory the server is
    /// running from.
    pub fn static_resources_location(
        mut self,
        static_resources_location: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.static_resources_location = static_resources_location.into();
        self
    }

    /// Constructs the service for use in a `.service()` call.
    pub fn finish(self) -> impl HttpServiceFactory {
        let index_file = self.index_file.into_owned();
        let static_resources_location = self.static_resources_location.into_owned();
        let static_resources_mount = self.static_resources_mount.into_owned();

        let files = {
            let index_file = index_file.clone();
            Files::new(&static_resources_mount, static_resources_location)
                // HACK: FilesService will try to read a directory listing unless index_file is provided
                // FilesService will fail to load the index_file and will then call our default_handler
                .index_file("extremely-unlikely-to-exist-!@$%^&*.txt")
                .default_handler(move |req| serve_index(req, index_file.clone()))
        };

        SpaService { index_file, files }
    }
}

#[derive(Debug)]
struct SpaService {
    index_file: String,
    files: Files,
}

impl HttpServiceFactory for SpaService {
    fn register(self, config: &mut actix_web::dev::AppService) {
        // let Files register its mount path as-is
        self.files.register(config);

        // also define a root prefix handler directed towards our SPA index
        let rdef = ResourceDef::root_prefix("");
        config.register_service(
            rdef,
            None,
            fn_service(move |req| serve_index(req, self.index_file.clone())),
            None,
        );
    }
}

async fn serve_index(
    req: ServiceRequest,
    index_file: String,
) -> Result<ServiceResponse, actix_web::Error> {
    trace!("serving default SPA page");
    let (req, _) = req.into_parts();
    let file = NamedFile::open_async(&index_file).await?;
    let res = file.into_response(&req);
    Ok(ServiceResponse::new(req, res))
}

impl Default for Spa {
    fn default() -> Self {
        Self {
            index_file: Cow::Borrowed("./index.html"),
            static_resources_mount: Cow::Borrowed("/"),
            static_resources_location: Cow::Borrowed("./"),
        }
    }
}
