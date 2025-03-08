//! For path segment extractor documentation, see [`Path`].

use actix_router::PathDeserializer;
use actix_utils::future::{Ready, ready};
use actix_web::{
    FromRequest, HttpRequest,
    dev::Payload,
    error::{Error, ErrorNotFound},
};
use derive_more::Display;
use serde::de;
use tracing::debug;

/// Extract typed data from request path segments.
///
/// Alternative to `web::Path` extractor from Actix Web that allows deconstruction, but omits the
/// implementation of `Deref`.
///
/// Unlike, [`HttpRequest::match_info`], this extractor will fully percent-decode dynamic segments,
/// including `/`, `%`, and `+`.
///
/// # Examples
/// ```
/// use actix_web::get;
/// use actix_web_lab::extract::Path;
///
/// // extract path info from "/{name}/{count}/index.html" into tuple
/// // {name}  - deserialize a String
/// // {count} - deserialize a u32
/// #[get("/{name}/{count}/index.html")]
/// async fn index(Path((name, count)): Path<(String, u32)>) -> String {
///     format!("Welcome {}! {}", name, count)
/// }
/// ```
///
/// Path segments also can be deserialized into any type that implements [`serde::Deserialize`].
/// Path segment labels will be matched with struct field names.
///
/// ```
/// use actix_web::get;
/// use actix_web_lab::extract::Path;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     name: String,
/// }
///
/// // extract `Info` from a path using serde
/// #[get("/{name}")]
/// async fn index(info: Path<Info>) -> String {
///     let info = info.into_inner();
///     format!("Welcome {}!", info.name)
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Display)]
pub struct Path<T>(pub T);

impl<T> Path<T> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl_more::impl_as_ref!(Path<T> => T);
impl_more::impl_from!(<T> in T => Path<T>);

/// See [here](#Examples) for example of usage as an extractor.
impl<T> FromRequest for Path<T>
where
    T: de::DeserializeOwned,
{
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(
            de::Deserialize::deserialize(PathDeserializer::new(req.match_info()))
                .map(Path)
                .map_err(move |err| {
                    debug!(
                        "Failed during Path extractor deserialization. \
                         Request path: {:?}",
                        req.path()
                    );

                    ErrorNotFound(err)
                }),
        )
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{dev::ResourceDef, test::TestRequest};
    use derive_more::Display;
    use serde::Deserialize;

    use super::*;

    #[derive(Deserialize, Debug, Display)]
    #[display("MyStruct({key}, {value})")]
    struct MyStruct {
        key: String,
        value: String,
    }

    #[derive(Deserialize)]
    struct Test2 {
        key: String,
        value: u32,
    }

    #[actix_web::test]
    async fn test_extract_path_single() {
        let resource = ResourceDef::new("/{value}/");

        let mut req = TestRequest::with_uri("/32/").to_srv_request();
        resource.capture_match_info(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        assert_eq!(
            Path::<i8>::from_request(&req, &mut pl)
                .await
                .unwrap()
                .into_inner(),
            32
        );
        assert!(Path::<MyStruct>::from_request(&req, &mut pl).await.is_err());
    }

    #[actix_web::test]
    async fn test_tuple_extract() {
        let resource = ResourceDef::new("/{key}/{value}/");

        let mut req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();
        resource.capture_match_info(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let (Path(res),) = <(Path<(String, String)>,)>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(res.0, "name");
        assert_eq!(res.1, "user1");

        let (Path(a), Path(b)) =
            <(Path<(String, String)>, Path<(String, String)>)>::from_request(&req, &mut pl)
                .await
                .unwrap();
        assert_eq!(a.0, "name");
        assert_eq!(a.1, "user1");
        assert_eq!(b.0, "name");
        assert_eq!(b.1, "user1");

        <()>::from_request(&req, &mut pl).await.unwrap();
    }

    #[actix_web::test]
    async fn test_request_extract() {
        let mut req = TestRequest::with_uri("/name/user1/?id=test").to_srv_request();

        let resource = ResourceDef::new("/{key}/{value}/");
        resource.capture_match_info(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let s = Path::<MyStruct>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(format!("{s}"), "MyStruct(name, user1)");
        assert_eq!(
            format!("{s:?}"),
            "Path(MyStruct { key: \"name\", value: \"user1\" })"
        );
        let mut s = s.into_inner();
        assert_eq!(s.key, "name");
        assert_eq!(s.value, "user1");
        s.value = "user2".to_string();
        assert_eq!(s.value, "user2");

        let Path(s) = Path::<(String, String)>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.0, "name");
        assert_eq!(s.1, "user1");

        let mut req = TestRequest::with_uri("/name/32/").to_srv_request();
        let resource = ResourceDef::new("/{key}/{value}/");
        resource.capture_match_info(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let s = Path::<Test2>::from_request(&req, &mut pl).await.unwrap();
        assert_eq!(s.as_ref().key, "name");
        let s = s.into_inner();
        assert_eq!(s.value, 32);

        let Path(s) = Path::<(String, u8)>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.0, "name");
        assert_eq!(s.1, 32);

        let s = Path::<Vec<String>>::from_request(&req, &mut pl)
            .await
            .unwrap();
        let s = s.into_inner();
        assert_eq!(s[0], "name".to_owned());
        assert_eq!(s[1], "32".to_owned());
    }

    #[actix_web::test]
    async fn paths_decoded() {
        let resource = ResourceDef::new("/{key}/{value}");
        let mut req = TestRequest::with_uri("/na%2Bme/us%2Fer%254%32").to_srv_request();
        resource.capture_match_info(req.match_info_mut());

        let (req, mut pl) = req.into_parts();
        let path_items = Path::<MyStruct>::from_request(&req, &mut pl).await.unwrap();
        let path_items = path_items.into_inner();
        assert_eq!(path_items.key, "na+me");
        assert_eq!(path_items.value, "us/er%42");
        assert_eq!(req.match_info().as_str(), "/na%2Bme/us%2Fer%2542");
    }
}
