/// Quickly write tests that check various parts of a `ServiceResponse`.
///
/// An async test must be used (e.g., `#[actix_web::test]`) if used to assert on response body.
///
/// # Examples
/// ```
/// use actix_web::{
///     dev::ServiceResponse, http::header::ContentType, test::TestRequest, HttpResponse,
/// };
/// use actix_web_lab::assert_response_matches;
///
/// # actix_web::rt::System::new().block_on(async {
/// let res = ServiceResponse::new(
///     TestRequest::default().to_http_request(),
///     HttpResponse::Created()
///         .insert_header(("date", "today"))
///         .insert_header(("set-cookie", "a=b"))
///         .body("Hello World!"),
/// );
///
/// assert_response_matches!(res, CREATED);
/// assert_response_matches!(res, CREATED; "date" => "today");
/// assert_response_matches!(res, CREATED; @raw "Hello World!");
///
/// let res = ServiceResponse::new(
///     TestRequest::default().to_http_request(),
///     HttpResponse::Created()
///         .insert_header(("date", "today"))
///         .insert_header(("set-cookie", "a=b"))
///         .body("Hello World!"),
/// );
///
/// assert_response_matches!(res, CREATED;
///     "date" => "today"
///     "set-cookie" => "a=b";
///     @raw "Hello World!"
/// );
///
/// let res = ServiceResponse::new(
///     TestRequest::default().to_http_request(),
///     HttpResponse::Created()
///         .content_type(ContentType::json())
///         .insert_header(("date", "today"))
///         .insert_header(("set-cookie", "a=b"))
///         .body(r#"{"abc":"123"}"#),
/// );
///
/// assert_response_matches!(res, CREATED; @json { "abc": "123" });
/// # });
/// ```
#[macro_export]
macro_rules! assert_response_matches {
    ($res:ident, $status:ident) => {{
        assert_eq!($res.status(), ::actix_web::http::StatusCode::$status)
    }};

    ($res:ident, $status:ident; $($hdr_name:expr => $hdr_val:expr)+) => {{
        assert_response_matches!($res, $status);

        $(
            assert_eq!(
                $res.headers().get(::actix_web::http::header::HeaderName::from_static($hdr_name)).unwrap(),
                &::actix_web::http::header::HeaderValue::from_static($hdr_val),
            );
        )+
    }};

    ($res:ident, $status:ident; @raw $payload:expr) => {{
        assert_response_matches!($res, $status);
        assert_eq!(::actix_web::test::read_body($res).await, $payload);
    }};

    ($res:ident, $status:ident; $($hdr_name:expr => $hdr_val:expr)+; @raw $payload:expr) => {{
        assert_response_matches!($res, $status; $($hdr_name => $hdr_val)+);
        assert_eq!(::actix_web::test::read_body($res).await, $payload);
    }};

    ($res:ident, $status:ident; @json $payload:tt) => {{
        assert_response_matches!($res, $status);
        assert_eq!(
            ::actix_web::test::read_body_json::<$crate::__reexports::serde_json::Value, _>($res).await,
            $crate::__reexports::serde_json::json!($payload),
        );
    }};
}

pub use assert_response_matches;

#[cfg(test)]
mod tests {
    use actix_web::{
        HttpResponse, dev::ServiceResponse, http::header::ContentType, test::TestRequest,
    };

    use super::*;

    #[actix_web::test]
    async fn response_matching() {
        let res = ServiceResponse::new(
            TestRequest::default().to_http_request(),
            HttpResponse::Created()
                .insert_header(("date", "today"))
                .insert_header(("set-cookie", "a=b"))
                .body("Hello World!"),
        );

        assert_response_matches!(res, CREATED);
        assert_response_matches!(res, CREATED; "date" => "today");
        assert_response_matches!(res, CREATED; @raw "Hello World!");

        let res = ServiceResponse::new(
            TestRequest::default().to_http_request(),
            HttpResponse::Created()
                .insert_header(("date", "today"))
                .insert_header(("set-cookie", "a=b"))
                .body("Hello World!"),
        );
        assert_response_matches!(res, CREATED;
            "date" => "today"
            "set-cookie" => "a=b";
            @raw "Hello World!"
        );

        let res = ServiceResponse::new(
            TestRequest::default().to_http_request(),
            HttpResponse::Created()
                .content_type(ContentType::json())
                .insert_header(("date", "today"))
                .insert_header(("set-cookie", "a=b"))
                .body(r#"{"abc":"123"}"#),
        );

        assert_response_matches!(res, CREATED; @json { "abc": "123" });
    }
}
