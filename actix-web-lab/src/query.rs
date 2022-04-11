//! For query parameter extractor documentation, see [`Query`].

use std::{
    fmt,
    future::{ready, Ready},
    ops,
};

use actix_web::{dev::Payload, error::QueryPayloadError, Error, FromRequest, HttpRequest};
use serde::de::DeserializeOwned;
use tracing::debug;

/// Extract typed information from the request's query.
///
/// This currently identical in purpose to the `Query` extractor in `actix-web` but it will be able
/// to track version bumps of `serde-urlencoded` more closely. This version also does away with the
/// custom error handler config.
///
/// To extract typed data from the URL query string, the inner type `T` must implement the
/// [`DeserializeOwned`] trait.
///
/// # Panics
/// A query string consists of unordered `key=value` pairs, therefore it cannot be decoded into any
/// type which depends upon data ordering (eg. tuples). Trying to do so will result in a panic.
///
/// # Examples
/// ```
/// use actix_web::get;
/// use actix_web_lab::extract::Query;
/// use serde::Deserialize;
///
/// #[derive(Debug, Deserialize)]
/// pub enum ResponseType {
///    Token,
///    Code
/// }
///
/// #[derive(Debug, Deserialize)]
/// pub struct AuthRequest {
///    id: u64,
///    response_type: ResponseType,
/// }
///
/// // Deserialize `AuthRequest` struct from query string.
/// // This handler gets called only if the request's query parameters contain both fields.
/// // A valid request path for this handler would be `/?id=64&response_type=Code"`.
/// #[get("/")]
/// async fn index(info: Query<AuthRequest>) -> String {
///     format!("Authorization request for id={} and type={:?}!", info.id, info.response_type)
/// }
///
/// // To access the entire underlying query struct, use `.into_inner()`.
/// #[get("/debug1")]
/// async fn debug1(info: Query<AuthRequest>) -> String {
///     dbg!("Authorization object = {:?}", info.into_inner());
///     "OK".to_string()
/// }
///
/// // Or use destructuring, which is equivalent to `.into_inner()`.
/// #[get("/debug2")]
/// async fn debug2(Query(info): Query<AuthRequest>) -> String {
///     dbg!("Authorization object = {:?}", info);
///     "OK".to_string()
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Query<T>(pub T);

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
    /// assert_eq!(numbers.get("one"), Some(&1));
    /// assert_eq!(numbers.get("two"), Some(&2));
    /// assert!(numbers.get("three").is_none());
    /// ```
    pub fn from_query(query_str: &str) -> Result<Self, QueryPayloadError> {
        serde_urlencoded::from_str::<T>(query_str)
            .map(Self)
            .map_err(QueryPayloadError::Deserialize)
    }
}

impl<T> ops::Deref for Query<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> ops::DerefMut for Query<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: fmt::Display> fmt::Display for Query<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// See [here](#examples) for example of usage as an extractor.
impl<T: DeserializeOwned> FromRequest for Query<T> {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        serde_urlencoded::from_str::<T>(req.query_string())
            .map(|val| ready(Ok(Query(val))))
            .unwrap_or_else(move |e| {
                let err = QueryPayloadError::Deserialize(e);

                debug!(
                    "Failed during Query extractor deserialization. \
                     Request path: {:?}",
                    req.path()
                );

                ready(Err(err.into()))
            })
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
        assert_eq!(
            format!("{}, {:?}", s, s),
            "test, Query(Id { id: \"test\" })"
        );

        s.id = "test1".to_string();
        let s = s.into_inner();
        assert_eq!(s.id, "test1");
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
        assert_eq!(
            format!("{}, {:?}", s, s),
            "test, Query(Id { id: \"test\" })"
        );

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
