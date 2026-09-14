// Keep watcher behavior aligned with the other TLS hot-reload crate.
// Each package contains this module so it can be published independently.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use arc_swap::ArcSwap;
use notify::{EventKind, RecursiveMode, Watcher as _};

use crate::{BuildError, CredentialError};

const DEBOUNCE: Duration = Duration::from_millis(100);
const RETRY_DELAY: Duration = Duration::from_millis(200);
const MAX_RETRIES: usize = 5;

/// A worker notification. Receiving notifications is optional.
#[derive(Debug)]
pub enum Event {
    /// A complete, validated pair was published.
    Reloaded,

    /// One attempt failed. The last valid pair remains active.
    ReloadFailed {
        /// One-based attempt number in this batch.
        attempt: usize,
        /// Cause of the failure.
        error: CredentialError,
    },

    /// The native watcher reported an error. Its coverage may be impaired.
    WatchFailed(notify::Error),
}

/// Owns the native watcher and reload worker.
///
/// Drop stops watching and joins the worker. This can wait for an in-progress file read or
/// validation, but interrupts debounce and retry waits. Keep this handle until server shutdown.
/// TLS configurations can outlive it and will retain the last valid credentials.
#[derive(Debug)]
pub struct Watcher {
    stop: Arc<AtomicBool>,
    wake: mpsc::SyncSender<()>,
    worker: Option<JoinHandle<()>>,
    events: Option<mpsc::Receiver<Event>>,
}

impl Watcher {
    /// Spawn a thread that calls `observe` for each reload event.
    ///
    /// Optional: watching starts in `Builder::build` even without an observer. The observer
    /// exits after the watcher is dropped and buffered events are drained. Callbacks must
    /// return for the thread to exit. Join the returned handle if you need to wait for it.
    /// Returns an error if the event stream was already taken or thread creation fails.
    pub fn spawn_observer(
        &mut self,
        mut observe: impl FnMut(Event) + Send + 'static,
    ) -> std::io::Result<JoinHandle<()>> {
        let events = self.take_events().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Event stream already taken",
            )
        })?;

        thread::Builder::new()
            .name(format!("{}-observer", env!("CARGO_PKG_NAME")))
            .spawn(move || {
                for event in events {
                    observe(event);
                }
            })
    }

    /// Take the optional reload event stream. Returns `None` after the first call.
    ///
    /// The stream buffers up to 64 events. If it is full, new events are dropped; credential
    /// rotation never waits for the observer. Drop the receiver if monitoring is not needed.
    pub fn take_events(&mut self) -> Option<mpsc::Receiver<Event>> {
        self.events.take()
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.wake.try_send(());

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn absolute_file(path: &Path) -> Result<PathBuf, BuildError> {
    let path = std::path::absolute(path)?;
    let name = path
        .file_name()
        .ok_or_else(|| BuildError::InvalidPath(path.clone()))?;

    // Canonicalize only the parent: replacing the file must not change the watch target.
    Ok(path
        .parent()
        .ok_or_else(|| BuildError::InvalidPath(path.clone()))?
        .canonicalize()?
        .join(name))
}

// All filesystem operations, setup, initial parsing, and reload validation run on this worker.
pub(crate) fn start<T, O>(
    cert: &Path,
    key: &Path,
    mut parse: impl FnMut(&[u8], &[u8]) -> Result<T, CredentialError> + Send + 'static,
    wrap: impl FnOnce(Arc<ArcSwap<T>>) -> Result<O, BuildError> + Send + 'static,
) -> Result<(O, Watcher), BuildError>
where
    T: Send + Sync + 'static,
    O: Send + 'static,
{
    let cert = cert.to_owned();
    let key = key.to_owned();

    let (wake, incoming) = mpsc::sync_channel(1);
    let (events, receiver) = mpsc::sync_channel(64);
    let (ready, initialized) = mpsc::sync_channel(1);

    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let notify_wake = wake.clone();

    let worker = thread::Builder::new()
        .name(env!("CARGO_PKG_NAME").into())
        .spawn(move || {
            let setup = (|| {
                let cert = absolute_file(&cert)?;
                let key = absolute_file(&key)?;

                let watched_cert = cert.clone();
                let watched_key = key.clone();
                let watch_events = events.clone();

                let mut watcher =
                    notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                        match event {
                            Ok(event)
                                if !matches!(event.kind, EventKind::Access(_))
                                    && (event.need_rescan()
                                        || event.paths.is_empty()
                                        || event
                                            .paths
                                            .iter()
                                            .any(|p| p == &watched_cert || p == &watched_key)) =>
                            {
                                let _ = notify_wake.try_send(());
                            }
                            Err(err) => {
                                let _ = watch_events.try_send(Event::WatchFailed(err));
                            }
                            _ => {}
                        }
                    })?;

                watcher.watch(cert.parent().unwrap(), RecursiveMode::NonRecursive)?;
                if cert.parent() != key.parent() {
                    watcher.watch(key.parent().unwrap(), RecursiveMode::NonRecursive)?;
                }

                let bytes = (
                    std::fs::read(&cert).map_err(CredentialError::Io)?,
                    std::fs::read(&key).map_err(CredentialError::Io)?,
                );
                let current = Arc::new(ArcSwap::from_pointee(parse(&bytes.0, &bytes.1)?));
                let output = wrap(Arc::clone(&current))?;

                Ok::<_, BuildError>((watcher, cert, key, bytes, current, output))
            })();

            let (_watcher, cert, key, mut last, current, output) = match setup {
                Ok(setup) => setup,
                Err(error) => {
                    let _ = ready.send(Err(error));
                    return;
                }
            };

            if ready.send(Ok(output)).is_err() {
                return;
            }

            while incoming.recv().is_ok() {
                if stopped.load(Ordering::Acquire) {
                    break;
                }

                let cap = Instant::now() + DEBOUNCE.saturating_mul(10);
                let mut deadline = Instant::now() + DEBOUNCE;

                loop {
                    if stopped.load(Ordering::Acquire) {
                        return;
                    }

                    match incoming.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    {
                        Ok(()) if Instant::now() < cap => {
                            deadline = cap.min(Instant::now() + DEBOUNCE);
                        }
                        _ => break,
                    }
                }

                for attempt in 0..=MAX_RETRIES {
                    if stopped.load(Ordering::Acquire) {
                        return;
                    }

                    let result = (|| {
                        let bytes = (
                            std::fs::read(&cert).map_err(CredentialError::Io)?,
                            std::fs::read(&key).map_err(CredentialError::Io)?,
                        );
                        if bytes == last {
                            return Ok(None);
                        }

                        let pair = parse(&bytes.0, &bytes.1)?;

                        Ok::<_, CredentialError>(Some((bytes, pair)))
                    })();

                    match result {
                        Ok(Some((bytes, pair))) => {
                            current.store(Arc::new(pair));
                            last = bytes;
                            let _ = events.try_send(Event::Reloaded);
                            break;
                        }
                        Ok(None) => break,
                        Err(error) => {
                            let _ = events.try_send(Event::ReloadFailed {
                                attempt: attempt + 1,
                                error,
                            });
                        }
                    }

                    if attempt == MAX_RETRIES {
                        break;
                    }

                    let deadline = Instant::now() + RETRY_DELAY;
                    while Instant::now() < deadline {
                        if stopped.load(Ordering::Acquire) {
                            return;
                        }

                        let _ = incoming
                            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
                    }
                }
            }
        })?;

    let watcher = Watcher {
        stop,
        wake,
        worker: Some(worker),
        events: Some(receiver),
    };

    let output = initialized
        .recv()
        .map_err(|_| BuildError::WorkerStopped)??;

    Ok((output, watcher))
}
