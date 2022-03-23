/// Create a `TestRequest` using a DSL that looks kinda like on-the-wire HTTP/1.x requests.
///
/// # Examples
/// ```
/// use actix_web::test::TestRequest;
/// use actix_web_lab::test_request;
///
/// let _req: TestRequest = test_request! {
///     POST "/";
///     "Origin" => "example.com"
///     "Access-Control-Request-Method" => "POST"
///     "Access-Control-Request-Headers" => "Content-Type, X-CSRF-TOKEN";
///     @json {"abc": "123"}
/// };
///
/// let _req: TestRequest = test_request! {
///     POST "/";
///     "Content-Type" => "application/json"
///     "Origin" => "example.com"
///     "Access-Control-Request-Method" => "POST"
///     "Access-Control-Request-Headers" => "Content-Type, X-CSRF-TOKEN";
///     @raw r#"{"abc": "123"}"#
/// };
/// ```
#[macro_export]
macro_rules! test_request {
    ($method:ident $uri:expr) => {{
        ::actix_web::test::TestRequest::default()
            .method(::actix_web::http::Method::$method)
            .uri($uri)
    }};

    ($method:ident $uri:expr; $($hdr_name:expr => $hdr_val:expr)+) => {{
        test_request!($method $uri)
            $(
                .insert_header(($hdr_name, $hdr_val))
            )+
    }};

    ($method:ident $uri:expr; $($hdr_name:expr => $hdr_val:expr)+; @json $payload:tt) => {{
        test_request!($method $uri; $($hdr_name => $hdr_val)+)
            .set_json($crate::__reexports::serde_json::json!($payload))
    }};

    ($method:ident $uri:expr; $($hdr_name:expr => $hdr_val:expr)+; @raw $payload:expr) => {{
        test_request!($method $uri; $($hdr_name => $hdr_val)+)
            .set_payload($payload)
    }};
}

pub use test_request;

#[cfg(test)]
mod tests {
    use actix_web::test::TestRequest;

    use super::*;

    #[test]
    fn request_builder() {
        let _req: TestRequest = test_request! {
            POST "/";
            "Origin" => "example.com"
            "Access-Control-Request-Method" => "POST"
            "Access-Control-Request-Headers" => "Content-Type, X-CSRF-TOKEN";
            @json { "abc": "123" }
        };

        let _req: TestRequest = test_request! {
            POST "/";
            "Content-Type" => "application/json"
            "Origin" => "example.com"
            "Access-Control-Request-Method" => "POST"
            "Access-Control-Request-Headers" => "Content-Type, X-CSRF-TOKEN";
            @raw r#"{"abc": "123"}"#
        };
    }
}
