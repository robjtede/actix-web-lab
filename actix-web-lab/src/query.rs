//! For query parameter extractor documentation, see [`Query`].

use std::{
    fmt,
    future::{Ready, ready},
};

use actix_web::{FromRequest, HttpRequest, ResponseError, dev::Payload, http::StatusCode};
use derive_more::Error;
use serde::de::DeserializeOwned;

/// Extract typed information from the request's query.
///
/// To extract typed data from the URL query string, the inner type `T` must implement the
/// [`DeserializeOwned`] trait.
///
/// # Differences From `actix_web::web::Query`
/// This extractor uses `serde_html_form` under-the-hood which supports multi-value items. These are
/// sent by HTML select inputs when multiple options are chosen and can be collected into a `Vec`.
///
/// This version also removes the custom error handler config; users should instead prefer to handle
/// errors using the explicit `Result<Query<T>, E>` extractor in their handlers.
///
/// # Panics
/// A query string consists of unordered `key=value` pairs, therefore it cannot be decoded into any
/// type which depends upon data ordering (eg. tuples). Trying to do so will result in a panic.
///
/// # Examples
/// ```
/// use actix_web::{Responder, get};
/// use actix_web_lab::extract::Query;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// #[serde(rename_all = "lowercase")]
/// enum LogType {
///     Reports,
///     Actions,
/// }
///
/// #[derive(Debug, Deserialize)]
/// pub struct LogsParams {
///     #[serde(rename = "type")]
///     log_type: u64,
///
///     #[serde(rename = "user")]
///     users: Vec<String>,
/// }
///
/// // Deserialize `LogsParams` struct from query string.
/// // This handler gets called only if the request's query parameters contain both fields.
/// // A valid request path for this handler would be `/logs?type=reports&user=foo&user=bar"`.
/// #[get("/logs")]
/// async fn index(info: Query<LogsParams>) -> impl Responder {
///     let LogsParams { log_type, users } = info.into_inner();
///     format!("Logs request for type={log_type} and user list={users:?}!")
/// }
///
/// // Or use destructuring, which is equivalent to `.into_inner()`.
/// #[get("/debug2")]
/// async fn debug2(Query(info): Query<LogsParams>) -> impl Responder {
///     dbg!("Authorization object = {info:?}");
///     "OK"
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Query<T>(pub T);

impl_more::impl_deref_and_mut!(<T> in Query<T> => T);
impl_more::forward_display!(<T> in Query<T>);

impl<T> Query<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: DeserializeOwned> Query<T> {
    /// Deserialize a `T` from the URL encoded query parameter string.
    ///
    /// ```
    /// # use std::collections::HashMap;
    /// # use actix_web_lab::extract::Query;
    /// let numbers = Query::<HashMap<String, u32>>::from_query("one=1&two=2").unwrap();
    ///
    /// assert_eq!(numbers.get("one"), Some(&1));
    /// assert_eq!(numbers.get("two"), Some(&2));
    /// assert!(numbers.get("three").is_none());
    /// ```
    pub fn from_query(query_str: &str) -> Result<Self, QueryDeserializeError> {
        let parser = form_urlencoded::parse(query_str.as_bytes());
        let de = serde_html_form::Deserializer::new(parser);

        serde_path_to_error::deserialize(de)
            .map(Self)
            .map_err(|err| QueryDeserializeError {
                path: err.path().clone(),
                source: err.into_inner(),
            })
    }
}

/// See [here](#examples) for example of usage as an extractor.
impl<T: DeserializeOwned> FromRequest for Query<T> {
    type Error = QueryDeserializeError;
    type Future = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(Self::from_query(req.query_string()).inspect_err(|err| {
            tracing::debug!(
                "Failed during Query extractor deserialization. \
                Request path: \"{}\". \
                Error path: \"{}\".",
                req.match_name().unwrap_or(req.path()),
                err.path(),
            );
        }))
    }
}

/// Deserialization errors that can occur during parsing query strings.
#[derive(Debug, Error)]
pub struct QueryDeserializeError {
    /// Path where deserialization error occurred.
    path: serde_path_to_error::Path,

    /// Deserialization error.
    source: serde_html_form::de::Error,
}

impl QueryDeserializeError {
    /// Returns the path at which the deserialization error occurred.
    pub fn path(&self) -> impl fmt::Display + '_ {
        &self.path
    }

    /// Returns the source error.
    pub fn source(&self) -> &serde_html_form::de::Error {
        &self.source
    }
}

impl fmt::Display for QueryDeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Query deserialization failed")?;

        if self.path.iter().len() > 0 {
            write!(f, " at path: {}", &self.path)?;
        }

        Ok(())
    }
}

impl ResponseError for QueryDeserializeError {
    fn status_code(&self) -> StatusCode {
        StatusCode::UNPROCESSABLE_ENTITY
    }
}

#[cfg(test)]
mod tests {
    use actix_web::test::TestRequest;
    use derive_more::Display;
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize, Debug, Display)]
    struct Id {
        id: String,
    }

    #[actix_web::test]
    async fn test_service_request_extract() {
        let req = TestRequest::with_uri("/name/user1/").to_srv_request();
        assert!(Query::<Id>::from_query(req.query_string()).is_err());

        let req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();
        let mut s = Query::<Id>::from_query(req.query_string()).unwrap();

        assert_eq!(s.id, "test");
        assert_eq!(format!("{s}, {s:?}"), "test, Query(Id { id: \"test\" })");

        s.id = "test1".to_string();
        let s = s.into_inner();
        assert_eq!(s.id, "test1");
    }

    #[actix_web::test]
    async fn extract_array() {
        #[derive(Debug, Deserialize)]
        struct Test {
            #[serde(rename = "user")]
            users: Vec<String>,
        }

        let req = TestRequest::with_uri("/?user=foo&user=bar").to_srv_request();
        let s = Query::<Test>::from_query(req.query_string()).unwrap();

        assert_eq!(s.users[0], "foo");
        assert_eq!(s.users[1], "bar");
    }

    #[actix_web::test]
    async fn test_request_extract() {
        let req = TestRequest::with_uri("/name/user1/").to_srv_request();
        let (req, mut pl) = req.into_parts();
        assert!(Query::<Id>::from_request(&req, &mut pl).await.is_err());

        let req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();
        let (req, mut pl) = req.into_parts();

        let mut s = Query::<Id>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.id, "test");
        assert_eq!(format!("{s}, {s:?}"), "test, Query(Id { id: \"test\" })");

        s.id = "test1".to_string();
        let s = s.into_inner();
        assert_eq!(s.id, "test1");
    }

    #[actix_web::test]
    #[should_panic]
    async fn test_tuple_panic() {
        let req = TestRequest::with_uri("/?one=1&two=2").to_srv_request();
        let (req, mut pl) = req.into_parts();

        Query::<(u32, u32)>::from_request(&req, &mut pl)
            .await
            .unwrap();
    }
}
