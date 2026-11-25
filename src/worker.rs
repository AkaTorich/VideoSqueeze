// Job queue: one thread converts files one at a time (encoders and scalers use the cores themselves).
// Progress and results go to the window as events; a stop moves the queue to a new generation.
use std::collections::VecDeque;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use videosqueeze::job::run::{self, JobResult, Reporter, CANCELLED};
use videosqueeze::job::settings::Settings;

pub struct Job {
    pub id: u64,
    pub path: String,
    pub settings: Settings,
}

#[derive(Default)]
pub struct Queue {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    items: Mutex<VecDeque<(u64, Job)>>,
    ready: Condvar,
    generation: AtomicU64,
}

impl Queue {
    pub fn push(&self, jobs: Vec<Job>) {
        let g = self.shared.generation.load(Ordering::SeqCst);
        let mut q = self.shared.items.lock().unwrap();
        q.extend(jobs.into_iter().map(|j| (g, j)));
        self.shared.ready.notify_all();
    }

    pub fn cancel_all(&self) {
        let mut q = self.shared.items.lock().unwrap();
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        q.clear();
    }
}

#[derive(Serialize, Clone)]
struct ProgressEvent {
    id: u64,
    stage: usize,
    progress: f64,
    speed: f64,
}

#[derive(Serialize, Clone)]
struct DoneEvent {
    id: u64,
    result: Option<JobResult>,
    error: Option<String>,
}

struct EventReporter<'a> {
    app: &'a AppHandle,
    shared: &'a Shared,
    id: u64,
    generation: u64,
    /// Last sent tenth of a percent and time — the window is not flooded.
    last: AtomicI64,
    last_at: Mutex<Instant>,
}

impl Reporter for EventReporter<'_> {
    fn progress(&self, stage: usize, fraction: f64, speed: f64) {
        let p = (fraction.clamp(0.0, 1.0) * 1000.0).floor() as i64 + stage as i64 * 10_000;
        let prev = self.last.load(Ordering::Relaxed);
        let mut at = self.last_at.lock().unwrap();
        if p != prev && (at.elapsed().as_millis() >= 80 || p / 10_000 != prev / 10_000) {
            self.last.store(p, Ordering::Relaxed);
            *at = Instant::now();
            let _ = self.app.emit("job-progress", ProgressEvent { id: self.id, stage, progress: fraction.clamp(0.0, 1.0), speed });
        }
    }

    fn cancelled(&self) -> bool {
        self.shared.generation.load(Ordering::Relaxed) != self.generation
    }
}

pub fn start(app: AppHandle) {
    let shared = app.state::<Queue>().shared.clone();
    std::thread::Builder::new()
        .name("videosqueeze-queue".into())
        .spawn(move || loop {
            let (generation, job) = {
                let mut q = shared.items.lock().unwrap();
                loop {
                    if let Some(item) = q.pop_front() {
                        break item;
                    }
                    q = shared.ready.wait(q).unwrap();
                }
            };
            if shared.generation.load(Ordering::SeqCst) != generation {
                continue;
            }
            let rep = EventReporter { app: &app, shared: &shared, id: job.id, generation, last: AtomicI64::new(-1), last_at: Mutex::new(Instant::now()) };
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| run::run(Path::new(&job.path), &job.settings, &rep)));
            let event = match outcome {
                Ok(Ok(result)) => DoneEvent { id: job.id, result: Some(result), error: None },
                Ok(Err(e)) => DoneEvent { id: job.id, result: None, error: Some(if e == CANCELLED { e } else { e.to_uppercase() }) },
                Err(_) => DoneEvent { id: job.id, result: None, error: Some("INTERNAL ERROR WHILE CONVERTING".into()) },
            };
            let _ = app.emit("job-done", event);
        })
        .expect("queue thread");
}
