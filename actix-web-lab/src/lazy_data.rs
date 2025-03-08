use std::{
    cell::Cell,
    fmt,
    future::{Future, Ready, ready},
    rc::Rc,
};

use actix_web::{Error, FromRequest, HttpRequest, dev, error};
use futures_core::future::LocalBoxFuture;
use tokio::sync::OnceCell;
use tracing::debug;

/// A lazy extractor for thread-local data.
///
/// Using `LazyData` as an extractor will not initialize the data; [`get`](Self::get) must be used.
pub struct LazyData<T> {
    inner: Rc<LazyDataInner<T>>,
}

struct LazyDataInner<T> {
    cell: OnceCell<T>,
    fut: Cell<Option<LocalBoxFuture<'static, T>>>,
}

impl<T> Clone for LazyData<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for LazyData<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Lazy")
            .field("cell", &self.inner.cell)
            .field("fut", &"..")
            .finish()
    }
}

impl<T> LazyData<T> {
    /// Constructs a new `LazyData` extractor with the given initialization function.
    ///
    /// Initialization functions must return a future that resolves to `T`.
    pub fn new<F, Fut>(init: F) -> LazyData<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T> + 'static,
    {
        Self {
            inner: Rc::new(LazyDataInner {
                cell: OnceCell::new(),
                fut: Cell::new(Some(Box::pin(init()))),
            }),
        }
    }

    /// Returns reference to result of lazy `T` value, initializing if necessary.
    pub async fn get(&self) -> &T {
        self.inner
            .cell
            .get_or_init(|| async move {
                match self.inner.fut.take() {
                    Some(fut) => fut.await,
                    None => panic!("LazyData instance has previously been poisoned"),
                }
            })
            .await
    }
}

impl<T: 'static> FromRequest for LazyData<T> {
    type Error = Error;
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_request(req: &HttpRequest, _: &mut dev::Payload) -> Self::Future {
        if let Some(lazy) = req.app_data::<LazyData<T>>() {
            ready(Ok(lazy.clone()))
        } else {
            debug!(
                "Failed to extract `LazyData<{}>` for `{}` handler. For the Data extractor to work \
                correctly, wrap the data with `LazyData::new()` and pass it to `App::app_data()`. \
                Ensure that types align in both the set and retrieve calls.",
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

    #[actix_web::test]
    async fn lazy_data() {
        let app = init_service(
            App::new()
                .app_data(LazyData::new(|| async { 10usize }))
                .service(web::resource("/").to(|_: LazyData<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let app = init_service(
            App::new()
                .app_data(LazyData::new(|| async {
                    actix_web::rt::time::sleep(Duration::from_millis(40)).await;
                    10usize
                }))
                .service(web::resource("/").to(|_: LazyData<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let app = init_service(
            App::new()
                .app_data(LazyData::new(|| async { 10u32 }))
                .service(web::resource("/").to(|_: LazyData<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[actix_web::test]
    async fn lazy_data_web_block() {
        let app = init_service(
            App::new()
                .app_data(LazyData::new(|| async {
                    web::block(|| std::thread::sleep(Duration::from_millis(40)))
                        .await
                        .unwrap();

                    10usize
                }))
                .service(web::resource("/").to(|_: LazyData<usize>| HttpResponse::Ok())),
        )
        .await;
        let req = TestRequest::default().to_request();
        let resp = call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
