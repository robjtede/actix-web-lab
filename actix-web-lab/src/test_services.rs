use actix_utils::future::ok;
use actix_web::{
    body::BoxBody,
    dev::{fn_service, Service, ServiceRequest, ServiceResponse},
    http::StatusCode,
    Error, HttpResponseBuilder,
};

/// Creates service that always responds with given status code and echoes request path as response
/// body.
pub fn echo_path_service(
    status_code: StatusCode,
) -> impl Service<ServiceRequest, Response = ServiceResponse<BoxBody>, Error = Error> {
    fn_service(move |req: ServiceRequest| {
        let path = req.path().to_owned();
        ok(req.into_response(HttpResponseBuilder::new(status_code).body(path)))
    })
}
