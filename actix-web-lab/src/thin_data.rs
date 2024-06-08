use std::any::type_name;

use actix_utils::future::{ready, Ready};
use actix_web::{dev::Payload, error, FromRequest, HttpRequest};
use derive_more::{AsMut, AsRef, Deref, DerefMut};
use tracing::log;

/// Application data wrapper and extractor for cheaply-cloned types.
///
/// Similar to Actix Web's `Data` wrapper but for `Clone` types that are already an `Arc` internally
/// or shares state using some other means when cloned.
///
/// Unlike `Data`, this wrapper clones `T` during extraction. Therefore, it is the user's
/// responsibility to ensure that clones of `T` do actually share the same state, otherwise state
/// may be unexpectedly different across multiple requests.
///
/// Note that if your type is an `Arc<T>` then it's recommended to use Actix Web's `Data::from(arc)`
/// conversion instead.
///
/// # Examples
///
/// ```
/// use actix_web::{web, App, HttpResponse, Responder};
/// use actix_web_lab::extract::ThinData;
///
/// // Use the `ThinData<T>` extractor to access a database connection pool.
/// async fn index(ThinData(db_pool): ThinData<DbPool>) -> impl Responder {
///     // database action ...
///
///     HttpResponse::Ok()
/// }
///
/// # type DbPool = ();
/// let db_pool = DbPool::default();
///
/// App::new()
///     .app_data(ThinData(db_pool.clone()))
///     .service(web::resource("/").get(index))
/// # ;
/// ```
#[derive(Debug, Clone, AsRef, AsMut, Deref, DerefMut)]
pub struct ThinData<T>(pub T);

impl<T: Clone + 'static> FromRequest for ThinData<T> {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        ready(req.app_data::<Self>().cloned().ok_or_else(|| {
            log::debug!(
                "Failed to extract `ThinData<{}>` for `{}` handler. For the ThinData extractor to work \
                correctly, wrap the data with `ThinData()` and pass it to `App::app_data()`. \
                Ensure that types align in both the set and retrieve calls.",
                type_name::<T>(),
                req.match_name().unwrap_or_else(|| req.path())
            );

            error::ErrorInternalServerError(
                "Requested application data is not configured correctly. \
                View/enable debug logs for more details.",
            )
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ops::AddAssign,
        sync::{Arc, Mutex},
    };

    use actix_web::{
        http::StatusCode,
        test::{call_service, init_service, TestRequest},
        web, App, HttpResponse,
    };

    use super::*;

    type TestT = Arc<Mutex<u32>>;

    #[actix_web::test]
    async fn thin_data() {
        let test_data = TestT::default();

        let app = init_service(App::new().app_data(ThinData(test_data.clone())).service(
            web::resource("/").to(|td: ThinData<TestT>| {
                td.lock().unwrap().add_assign(1);
                HttpResponse::Ok()
            }),
        ))
        .await;

        for _ in 0..3 {
            let req = TestRequest::default().to_request();
            let resp = call_service(&app, req).await;
            assert_eq!(resp.status(), StatusCode::OK);
        }

        assert_eq!(*test_data.lock().unwrap(), 3);
    }

    #[actix_web::test]
    async fn thin_data_missing() {
        let app = init_service(
            App::new().service(web::resource("/").to(|_: ThinData<u32>| HttpResponse::Ok())),
        )
        .await;

        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
