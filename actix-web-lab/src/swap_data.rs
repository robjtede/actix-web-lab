use std::sync::Arc;

use actix_utils::future::{Ready, ready};
use actix_web::{Error, FromRequest, HttpRequest, dev, error};
use arc_swap::{ArcSwap, Guard};
use tracing::debug;

/// A wrapper around `ArcSwap` that can be used as an extractor.
///
/// Can serve as a replacement for `Data<RwLock<T>>` in certain situations.
///
/// Currently exposes some internals of `arc-swap` and may change in the future.
#[derive(Debug)]
pub struct SwapData<T> {
    swap: Arc<ArcSwap<T>>,
}

impl<T: Send + Sync> SwapData<T> {
    /// Constructs new swappable data item.
    pub fn new(item: T) -> Self {
        Self {
            swap: Arc::new(ArcSwap::new(Arc::new(item))),
        }
    }

    /// Returns a temporary access guard to the wrapped data item.
    ///
    /// Implements `Deref` for read access to the inner data item.
    pub fn load(&self) -> Guard<Arc<T>> {
        self.swap.load()
    }

    /// Replaces the value inside this instance.
    ///
    /// Further `load`s will yield the new value.
    pub fn store(&self, item: T) {
        self.swap.store(Arc::new(item))
    }
}

impl<T> Clone for SwapData<T> {
    fn clone(&self) -> Self {
        Self {
            swap: Arc::clone(&self.swap),
        }
    }
}

impl<T: 'static> FromRequest for SwapData<T> {
    type Error = Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _pl: &mut dev::Payload) -> Self::Future {
        if let Some(data) = req.app_data::<SwapData<T>>() {
            ready(Ok(SwapData {
                swap: Arc::clone(&data.swap),
            }))
        } else {
            debug!(
                "Failed to extract `SwapData<{}>` for `{}` handler. For the Data extractor to work \
                correctly, wrap the data with `SwapData::new()` and pass it to `App::app_data()`. \
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
    use actix_web::test::TestRequest;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct NonCopy(u32);

    #[actix_web::test]
    async fn deref() {
        let data = SwapData::new(NonCopy(42));
        let inner_data = data.load();
        let _inner_data: &NonCopy = &inner_data;
    }

    #[actix_web::test]
    async fn extract_success() {
        let data = SwapData::new(NonCopy(42));

        let req = TestRequest::default().app_data(data).to_http_request();
        let extracted_data = SwapData::<NonCopy>::extract(&req).await.unwrap();

        assert_eq!(**extracted_data.load(), NonCopy(42));
    }

    #[actix_web::test]
    async fn extract_fail() {
        let req = TestRequest::default().to_http_request();
        SwapData::<()>::extract(&req).await.unwrap_err();
    }

    #[actix_web::test]
    async fn store_and_reload() {
        let data = SwapData::new(NonCopy(42));
        let initial_data = Guard::into_inner(data.load());

        let req = TestRequest::default().app_data(data).to_http_request();

        // first load in handler loads initial value
        let extracted_data = SwapData::<NonCopy>::extract(&req).await.unwrap();
        assert_eq!(**extracted_data.load(), NonCopy(42));

        // change data
        extracted_data.store(NonCopy(80));

        // next load in handler loads new value
        let extracted_data = SwapData::<NonCopy>::extract(&req).await.unwrap();
        assert_eq!(**extracted_data.load(), NonCopy(80));

        // initial extracted data stays the same
        assert_eq!(*initial_data, NonCopy(42));
    }
}
