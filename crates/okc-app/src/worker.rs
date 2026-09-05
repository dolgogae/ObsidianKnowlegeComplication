//! Bounded single-operation worker for responsive terminal applications.

use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use okc_core::CancellationToken;

use crate::{OperationControl, ProgressEvent, ProgressObserver, Result};

type WorkerJob = Box<dyn FnOnce(OperationControl) -> Result<String> + Send + 'static>;

enum WorkerCommand {
    Run {
        operation: String,
        cancellation: CancellationToken,
        job: WorkerJob,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    Progress(ProgressEvent),
    Finished {
        operation: String,
        result: std::result::Result<String, String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    Requested,
    PublicationBarrier,
    Idle,
}

pub struct Worker {
    command_tx: SyncSender<WorkerCommand>,
    event_rx: Receiver<WorkerEvent>,
    active: Arc<AtomicBool>,
    publication_barrier: Arc<AtomicBool>,
    cancellation: Arc<Mutex<Option<CancellationToken>>>,
    join: Option<JoinHandle<()>>,
}

impl Debug for Worker {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Worker")
            .field("active", &self.active.load(Ordering::Acquire))
            .field(
                "publication_barrier",
                &self.publication_barrier.load(Ordering::Acquire),
            )
            .finish_non_exhaustive()
    }
}

impl Worker {
    pub fn spawn() -> Self {
        let (command_tx, command_rx) = sync_channel::<WorkerCommand>(1);
        let (event_tx, event_rx) = sync_channel::<WorkerEvent>(64);
        let active = Arc::new(AtomicBool::new(false));
        let publication_barrier = Arc::new(AtomicBool::new(false));
        let cancellation = Arc::new(Mutex::new(None));
        let thread_active = Arc::clone(&active);
        let thread_barrier = Arc::clone(&publication_barrier);
        let thread_cancellation = Arc::clone(&cancellation);
        let join = thread::Builder::new()
            .name("okc-application-worker".into())
            .spawn(move || {
                while let Ok(command) = command_rx.recv() {
                    match command {
                        WorkerCommand::Shutdown => break,
                        WorkerCommand::Run {
                            operation,
                            cancellation,
                            job,
                        } => {
                            thread_barrier.store(false, Ordering::Release);
                            let observer = Arc::new(ChannelObserver {
                                sender: event_tx.clone(),
                                publication_barrier: Arc::clone(&thread_barrier),
                            });
                            let control = OperationControl {
                                cancellation,
                                observer,
                            };
                            let result = job(control).map_err(|error| error.to_string());
                            let _ = event_tx.send(WorkerEvent::Finished { operation, result });
                            thread_active.store(false, Ordering::Release);
                            thread_barrier.store(false, Ordering::Release);
                            *thread_cancellation.lock().expect("worker cancellation") = None;
                        }
                    }
                }
            })
            .expect("OKC worker thread creation failed");
        Self {
            command_tx,
            event_rx,
            active,
            publication_barrier,
            cancellation,
            join: Some(join),
        }
    }

    pub fn submit<F>(&self, operation: impl Into<String>, job: F) -> bool
    where
        F: FnOnce(OperationControl) -> Result<String> + Send + 'static,
    {
        if self
            .active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        let cancellation = CancellationToken::default();
        *self.cancellation.lock().expect("worker cancellation") = Some(cancellation.clone());
        let command = WorkerCommand::Run {
            operation: operation.into(),
            cancellation,
            job: Box::new(job),
        };
        if self.command_tx.send(command).is_err() {
            self.active.store(false, Ordering::Release);
            *self.cancellation.lock().expect("worker cancellation") = None;
            return false;
        }
        true
    }

    pub fn try_recv(&self) -> Option<WorkerEvent> {
        self.event_rx.try_recv().ok()
    }

    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    pub fn cancel(&self) -> CancelOutcome {
        if !self.is_active() {
            return CancelOutcome::Idle;
        }
        if self.publication_barrier.load(Ordering::Acquire) {
            return CancelOutcome::PublicationBarrier;
        }
        if let Some(token) = self
            .cancellation
            .lock()
            .expect("worker cancellation")
            .as_ref()
        {
            token.cancel();
            CancelOutcome::Requested
        } else {
            CancelOutcome::Idle
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if let Some(token) = self
            .cancellation
            .lock()
            .expect("worker cancellation")
            .as_ref()
        {
            token.cancel();
        }
        // If a freshly submitted job still occupies the single command slot,
        // `try_send` can lose the shutdown marker and strand the worker on recv.
        let _ = self.command_tx.send(WorkerCommand::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

struct ChannelObserver {
    sender: SyncSender<WorkerEvent>,
    publication_barrier: Arc<AtomicBool>,
}

impl ProgressObserver for ChannelObserver {
    fn observe(&self, event: &ProgressEvent) {
        if event.phase == crate::OperationPhase::Publishing {
            self.publication_barrier.store(true, Ordering::Release);
        }
        match self.sender.try_send(WorkerEvent::Progress(event.clone())) {
            Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;
    use crate::{OperationKind, OperationPhase};

    #[test]
    fn worker_is_bounded_single_operation_and_forwards_cancellation() {
        let worker = Worker::spawn();
        let (started_tx, started_rx) = mpsc::channel();
        assert!(worker.submit("fixture", move |control| {
            started_tx.send(()).expect("started");
            while !control.cancellation.is_cancelled() {
                thread::yield_now();
            }
            Err(crate::AppError::InvalidProject("cancelled".into()))
        }));
        started_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("worker started");
        assert!(!worker.submit("second", |_| Ok("unexpected".into())));
        assert_eq!(worker.cancel(), CancelOutcome::Requested);
        loop {
            if matches!(worker.try_recv(), Some(WorkerEvent::Finished { .. })) {
                break;
            }
            thread::yield_now();
        }
    }

    #[test]
    fn publication_progress_closes_the_cancellation_window() {
        let worker = Worker::spawn();
        let (release_tx, release_rx) = mpsc::channel();
        assert!(worker.submit("publish", move |control| {
            control.observer.observe(&ProgressEvent {
                operation: OperationKind::Compile,
                phase: OperationPhase::Publishing,
                completed: 0,
                total: Some(1),
                current_item: None,
            });
            release_rx.recv().expect("release");
            Ok("done".into())
        }));
        loop {
            if matches!(worker.try_recv(), Some(WorkerEvent::Progress(_))) {
                break;
            }
            thread::yield_now();
        }
        assert_eq!(worker.cancel(), CancelOutcome::PublicationBarrier);
        release_tx.send(()).expect("release");
    }
}
