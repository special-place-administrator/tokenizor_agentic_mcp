use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::thread::{self, JoinHandle};

use anyhow::Result;

use super::store::{
    AnalyticsObservation, AnalyticsScope, AnalyticsStatus, AnalyticsStore, AnalyticsWriteOutcome,
    bounded_text,
};

pub const DEFAULT_ANALYTICS_QUEUE_CAPACITY: usize = 1024;
pub const MAX_ANALYTICS_QUEUE_CAPACITY: usize = 4096;
pub const MAX_ANALYTICS_QUEUE_ERROR_BYTES: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyticsEnqueueOutcome {
    Disabled(AnalyticsStatus),
    Enqueued,
    DroppedFull,
    FailedClosed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsQueueStatus {
    pub disabled: bool,
    pub configured_scope: AnalyticsScope,
    pub capacity: usize,
    pub enqueued: u64,
    pub recorded: u64,
    pub disabled_events: u64,
    pub dropped_full: u64,
    pub send_failures: u64,
    pub write_failures: u64,
    pub last_writer_error: Option<String>,
}

pub trait AnalyticsWriter: Send + 'static {
    fn write(&mut self, observation: &AnalyticsObservation) -> Result<AnalyticsWriteOutcome>;
}

#[derive(Clone)]
pub struct AnalyticsRecorder {
    inner: Arc<AnalyticsRecorderInner>,
}

enum AnalyticsRecorderInner {
    Disabled {
        db_path: PathBuf,
        counters: Arc<AnalyticsQueueCounters>,
    },
    Enabled(EnabledAnalyticsRecorder),
    Failed {
        configured_scope: AnalyticsScope,
        capacity: usize,
        counters: Arc<AnalyticsQueueCounters>,
    },
}

struct EnabledAnalyticsRecorder {
    configured_scope: AnalyticsScope,
    capacity: usize,
    sender: Option<SyncSender<AnalyticsObservation>>,
    counters: Arc<AnalyticsQueueCounters>,
    worker: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct AnalyticsQueueCounters {
    enqueued: AtomicU64,
    recorded: AtomicU64,
    disabled_events: AtomicU64,
    dropped_full: AtomicU64,
    send_failures: AtomicU64,
    write_failures: AtomicU64,
    last_writer_error: std::sync::Mutex<Option<String>>,
}

struct StoreAnalyticsWriter {
    store: AnalyticsStore,
}

impl AnalyticsRecorder {
    pub fn disabled(db_path: impl AsRef<Path>) -> Self {
        Self {
            inner: Arc::new(AnalyticsRecorderInner::Disabled {
                db_path: db_path.as_ref().to_path_buf(),
                counters: Arc::new(AnalyticsQueueCounters::new()),
            }),
        }
    }

    pub fn start(store: AnalyticsStore, configured_scope: AnalyticsScope, capacity: usize) -> Self {
        Self::start_with_writer(configured_scope, capacity, StoreAnalyticsWriter { store })
    }

    #[doc(hidden)]
    pub fn start_with_writer_for_tests<W>(
        configured_scope: AnalyticsScope,
        capacity: usize,
        writer: W,
    ) -> Self
    where
        W: AnalyticsWriter,
    {
        Self::start_with_writer(configured_scope, capacity, writer)
    }

    pub fn enqueue(&self, observation: AnalyticsObservation) -> AnalyticsEnqueueOutcome {
        match self.inner.as_ref() {
            AnalyticsRecorderInner::Disabled { db_path, counters } => {
                counters.disabled_events.fetch_add(1, Ordering::Relaxed);
                AnalyticsEnqueueOutcome::Disabled(AnalyticsStatus::Disabled {
                    db_path: db_path.clone(),
                })
            }
            AnalyticsRecorderInner::Enabled(enabled) => enabled.enqueue(observation),
            AnalyticsRecorderInner::Failed { counters, .. } => {
                counters.send_failures.fetch_add(1, Ordering::Relaxed);
                AnalyticsEnqueueOutcome::FailedClosed
            }
        }
    }

    pub fn status(&self) -> AnalyticsQueueStatus {
        match self.inner.as_ref() {
            AnalyticsRecorderInner::Disabled { counters, .. } => {
                counters.snapshot(true, AnalyticsScope::Disabled, 0)
            }
            AnalyticsRecorderInner::Enabled(enabled) => enabled.status(),
            AnalyticsRecorderInner::Failed {
                configured_scope,
                capacity,
                counters,
            } => counters.snapshot(false, configured_scope.clone(), *capacity),
        }
    }

    pub fn configured_scope(&self) -> AnalyticsScope {
        match self.inner.as_ref() {
            AnalyticsRecorderInner::Disabled { .. } => AnalyticsScope::Disabled,
            AnalyticsRecorderInner::Enabled(enabled) => enabled.configured_scope.clone(),
            AnalyticsRecorderInner::Failed {
                configured_scope, ..
            } => configured_scope.clone(),
        }
    }

    fn start_with_writer<W>(
        configured_scope: AnalyticsScope,
        capacity: usize,
        mut writer: W,
    ) -> Self
    where
        W: AnalyticsWriter,
    {
        let capacity = normalize_capacity(capacity);
        let (sender, receiver) = sync_channel::<AnalyticsObservation>(capacity);
        let counters = Arc::new(AnalyticsQueueCounters::new());
        let writer_counters = Arc::clone(&counters);
        let worker = thread::Builder::new()
            .name("symforge-analytics-writer".to_string())
            .spawn(move || {
                while let Ok(observation) = receiver.recv() {
                    match writer.write(&observation) {
                        Ok(AnalyticsWriteOutcome::Recorded { .. }) => {
                            writer_counters.recorded.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(AnalyticsWriteOutcome::Disabled(_)) => {
                            writer_counters
                                .disabled_events
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        Err(error) => {
                            writer_counters
                                .write_failures
                                .fetch_add(1, Ordering::Relaxed);
                            writer_counters.set_last_writer_error(error.to_string());
                        }
                    }
                }
            });

        match worker {
            Ok(worker) => Self {
                inner: Arc::new(AnalyticsRecorderInner::Enabled(EnabledAnalyticsRecorder {
                    configured_scope,
                    capacity,
                    sender: Some(sender),
                    counters,
                    worker: Some(worker),
                })),
            },
            Err(error) => {
                counters.write_failures.fetch_add(1, Ordering::Relaxed);
                counters.set_last_writer_error(format!("spawn analytics writer: {error}"));
                Self {
                    inner: Arc::new(AnalyticsRecorderInner::Failed {
                        configured_scope,
                        capacity,
                        counters,
                    }),
                }
            }
        }
    }
}

impl EnabledAnalyticsRecorder {
    fn enqueue(&self, observation: AnalyticsObservation) -> AnalyticsEnqueueOutcome {
        let Some(sender) = &self.sender else {
            self.counters.send_failures.fetch_add(1, Ordering::Relaxed);
            return AnalyticsEnqueueOutcome::FailedClosed;
        };

        match sender.try_send(observation.bounded_for_queue()) {
            Ok(()) => {
                self.counters.enqueued.fetch_add(1, Ordering::Relaxed);
                AnalyticsEnqueueOutcome::Enqueued
            }
            Err(TrySendError::Full(_)) => {
                self.counters.dropped_full.fetch_add(1, Ordering::Relaxed);
                AnalyticsEnqueueOutcome::DroppedFull
            }
            Err(TrySendError::Disconnected(_)) => {
                self.counters.send_failures.fetch_add(1, Ordering::Relaxed);
                AnalyticsEnqueueOutcome::FailedClosed
            }
        }
    }

    fn status(&self) -> AnalyticsQueueStatus {
        self.counters
            .snapshot(false, self.configured_scope.clone(), self.capacity)
    }
}

impl Drop for EnabledAnalyticsRecorder {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take()
            && worker.join().is_err()
        {
            self.counters.write_failures.fetch_add(1, Ordering::Relaxed);
            self.counters
                .set_last_writer_error("analytics writer thread panicked");
        }
    }
}

impl AnalyticsQueueCounters {
    fn new() -> Self {
        Self {
            enqueued: AtomicU64::new(0),
            recorded: AtomicU64::new(0),
            disabled_events: AtomicU64::new(0),
            dropped_full: AtomicU64::new(0),
            send_failures: AtomicU64::new(0),
            write_failures: AtomicU64::new(0),
            last_writer_error: std::sync::Mutex::new(None),
        }
    }

    fn snapshot(
        &self,
        disabled: bool,
        configured_scope: AnalyticsScope,
        capacity: usize,
    ) -> AnalyticsQueueStatus {
        AnalyticsQueueStatus {
            disabled,
            configured_scope,
            capacity,
            enqueued: self.enqueued.load(Ordering::Relaxed),
            recorded: self.recorded.load(Ordering::Relaxed),
            disabled_events: self.disabled_events.load(Ordering::Relaxed),
            dropped_full: self.dropped_full.load(Ordering::Relaxed),
            send_failures: self.send_failures.load(Ordering::Relaxed),
            write_failures: self.write_failures.load(Ordering::Relaxed),
            last_writer_error: self
                .last_writer_error
                .lock()
                .expect("analytics queue error mutex poisoned")
                .clone(),
        }
    }

    fn set_last_writer_error(&self, error: impl AsRef<str>) {
        let bounded = bounded_text(error.as_ref(), MAX_ANALYTICS_QUEUE_ERROR_BYTES);
        *self
            .last_writer_error
            .lock()
            .expect("analytics queue error mutex poisoned") = Some(bounded);
    }
}

impl AnalyticsWriter for StoreAnalyticsWriter {
    fn write(&mut self, observation: &AnalyticsObservation) -> Result<AnalyticsWriteOutcome> {
        self.store.record(observation)
    }
}

fn normalize_capacity(capacity: usize) -> usize {
    capacity.clamp(1, MAX_ANALYTICS_QUEUE_CAPACITY)
}
