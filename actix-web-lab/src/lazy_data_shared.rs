use std::{
    fmt,
    future::{Future, Ready, ready},
    sync::Arc,
};

use actix_web::{Error, FromRequest, HttpRequest, dev, error};
use futures_core::future::BoxFuture;
use tokio::sync::{Mutex, OnceCell};
use tracing::debug;

/// A lazy extractor for globally shared data.
///
/// Unlike, [`LazyData`], this type implements [`Send`] and [`Sync`].
///
/// Using `SharedLazyData` as an extractor will not initialize the data; [`get`](Self::get) must be
/// used.
///
/// [`LazyData`]: crate::extract::LazyData
pub struct LazyDataShared<T> {
    inner: Arc<LazyDataSharedInner<T>>,
}

struct LazyDataSharedInner<T> {
    cell: OnceCell<T>,
    fut: Mutex<Option<BoxFuture<'static, T>>>,
}

impl<T> Clone for LazyDataShared<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for LazyDataShared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { inner } = self;
        let LazyDataSharedInner { cell, fut: _ } = &**inner;

        f.debug_struct("SharedLazyData")
            .field("cell", &cell)
            .field("fut", &"..")
            .finish()
    }
}

impl<T> LazyDataShared<T> {
    /// Constructs a new `LazyData` extractor with the given initialization function.
    ///
    /// Initialization functions must return a future that resolves to `T`.
    pub fn new<F, Fut>(init: F) -> LazyDataShared<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T> + Send + 'static,
    {
        Self {
            inner: Arc::new(LazyDataSharedInner {
                cell: OnceCell::new(),
                fut: Mutex::new(Some(Box::pin(init()))),
            }),
        }
    }

    /// Returns reference to result of lazy `T` value, initializing if necessary.
    pub async fn get(&self) -> &T {
        self.inner
            .cell
            .get_or_init(|| async move {
                match &mut *self.inner.fut.lock().await {
                    Some(fut) => fut.await,
                    None => panic!("LazyData instance has previously been poisoned"),
                }
            })
            .await
    }
}

impl<T: 'static> FromRequest for LazyDataShared<T> {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut dev::Payload) -> Self::Future {
        if let Some(lazy) = req.app_data::<LazyDataShared<T>>() {
            ready(Ok(lazy.clone()))
        } else {
            debug!(
                "Failed to extract `SharedLazyData<{}>` for `{}` handler. For the Data extractor to \
                work correctly, wrap the data with `SharedLazyData::new()` and pass it to \
                `App::app_data()`. Ensure that types align in both the set and retrieve calls.",
                core::any::type_name::<T>(),
                req.match_name().unwrap_or_else(|| req.path())
            );

            ready(Err(error::ErrorInternalServerError(
                "Requested application data is not configured correctly. \
                View/enable debug logs for more details.",
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use actix_web::{
        App, HttpResponse,
        http::StatusCode,
        test::{TestRequest, call_service, init_service},
        web,
    };

    use super::*;

    static_assertions::assert_impl_all!(LazyDataShared<()>: Send, Sync);

    #[actix_web::test]
    async fn lazy_data() {
        let app = init_service(
            App::new()
                .app_data(LazyDataShared::new(|| async { 10usize }))
                .service(web::resource("/").to(|_: LazyDataShared<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let app = init_service(
            App::new()
                .app_data(LazyDataShared::new(|| async {
                    actix_web::rt::time::sleep(Duration::from_millis(40)).await;
                    10_usize
                }))
                .service(web::resource("/").to(
                    #[expect(clippy::async_yields_async)]
                    |lazy_num: LazyDataShared<usize>| async move {
                        if *lazy_num.get().await == 10 {
                            HttpResponse::Ok()
                        } else {
                            HttpResponse::InternalServerError()
                        }
                    },
                )),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(StatusCode::OK, resp.status());

        let app = init_service(
            App::new()
                .app_data(LazyDataShared::new(|| async { 10u32 }))
                .service(web::resource("/").to(|_: LazyDataShared<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(StatusCode::INTERNAL_SERVER_ERROR, resp.status());
    }

    #[actix_web::test]
    async fn lazy_data_web_block() {
        let app = init_service(
            App::new()
                .app_data(LazyDataShared::new(|| async {
                    web::block(|| std::thread::sleep(Duration::from_millis(40)))
                        .await
                        .unwrap();

                    10usize
                }))
                .service(web::resource("/").to(|_: LazyDataShared<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(StatusCode::OK, resp.status());
    }
}
