//! Panic reporter middleware.
//!
//! See [`PanicReporter`] for docs.

use std::{
    any::Any,
    future::{Ready, ready},
    panic::{self, AssertUnwindSafe},
    rc::Rc,
};

use actix_web::dev::{Service, Transform, forward_ready};
use futures_core::future::LocalBoxFuture;
use futures_util::FutureExt as _;

type PanicCallback = Rc<dyn Fn(&(dyn Any + Send))>;

/// A middleware that triggers a callback when the worker is panicking.
///
/// Mostly useful for logging or metrics publishing. The callback received the object with which
/// panic was originally invoked to allow down-casting.
///
/// # Examples
///
/// ```no_run
/// # use actix_web::App;
/// use actix_web_lab::middleware::PanicReporter;
/// # mod metrics {
/// #   macro_rules! increment_counter {
/// #       ($tt:tt) => {{}};
/// #   }
/// #   pub(crate) use increment_counter;
/// # }
///
/// App::new().wrap(PanicReporter::new(|_| metrics::increment_counter!("panic")))
///     # ;
/// ```
#[derive(Clone)]
pub struct PanicReporter {
    cb: PanicCallback,
}

impl PanicReporter {
    /// Constructs new panic reporter middleware with `callback`.
    pub fn new(callback: impl Fn(&(dyn Any + Send)) + 'static) -> Self {
        Self {
            cb: Rc::new(callback),
        }
    }
}

impl std::fmt::Debug for PanicReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanicReporter")
            .field("cb", &"<callback>")
            .finish()
    }
}

impl<S, Req> Transform<S, Req> for PanicReporter
where
    S: Service<Req>,
    S::Future: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Transform = PanicReporterMiddleware<S>;
    type InitError = ();
    type Future = Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        ready(Ok(PanicReporterMiddleware {
            service: Rc::new(service),
            cb: Rc::clone(&self.cb),
        }))
    }
}

/// Middleware service implementation for [`PanicReporter`].
#[doc(hidden)]
#[allow(missing_debug_implementations)]
pub struct PanicReporterMiddleware<S> {
    service: Rc<S>,
    cb: PanicCallback,
}

impl<S, Req> Service<Req> for PanicReporterMiddleware<S>
where
    S: Service<Req>,
    S::Future: 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = LocalBoxFuture<'static, Result<S::Response, S::Error>>;

    forward_ready!(service);

    fn call(&self, req: Req) -> Self::Future {
        let cb = Rc::clone(&self.cb);

        // catch panics in service call
        AssertUnwindSafe(self.service.call(req))
            .catch_unwind()
            .map(move |maybe_res| match maybe_res {
                Ok(res) => res,
                Err(panic_err) => {
                    // invoke callback with panic arg
                    (cb)(&panic_err);

                    // continue unwinding
                    panic::resume_unwind(panic_err)
                }
            })
            .boxed_local()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use actix_web::{
        App,
        dev::Service as _,
        test,
        web::{self, ServiceConfig},
    };

    use super::*;

    fn configure_test_app(cfg: &mut ServiceConfig) {
        cfg.route("/", web::get().to(|| async { "content" })).route(
            "/disco",
            #[allow(unreachable_code)]
            web::get().to(|| async {
                panic!("the disco");
                ""
            }),
        );
    }

    #[actix_web::test]
    async fn report_when_panics_occur() {
        let triggered = Arc::new(AtomicBool::new(false));

        let app = App::new()
            .wrap(PanicReporter::new({
                let triggered = Arc::clone(&triggered);
                move |_| {
                    triggered.store(true, Ordering::SeqCst);
                }
            }))
            .configure(configure_test_app);

        let app = test::init_service(app).await;

        let req = test::TestRequest::with_uri("/").to_request();
        assert!(app.call(req).await.is_ok());
        assert!(!triggered.load(Ordering::SeqCst));

        let req = test::TestRequest::with_uri("/disco").to_request();
        assert!(
            AssertUnwindSafe(app.call(req))
                .catch_unwind()
                .await
                .is_err()
        );
        assert!(triggered.load(Ordering::SeqCst));
    }
}
