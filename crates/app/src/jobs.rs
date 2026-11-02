//! Background tasks: every document operation runs on a worker thread so the UI never freezes.
use eframe::egui;
use udpt_core::Ctx;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};

enum Msg<T> {
    Progress(f32, String),
    Done(Result<T, String>),
}

pub struct Task<T> {
    rx: Receiver<Msg<T>>,
    cancel: Arc<AtomicBool>,
    pub progress: f32,
    pub label: String,
}

impl<T: Send + 'static> Task<T> {
    pub fn spawn(egui_ctx: &egui::Context, f: impl FnOnce(Ctx) -> anyhow::Result<T> + Send + 'static) -> Self {
        let (tx, rx) = channel::<Msg<T>>();
        let cancel = Arc::new(AtomicBool::new(false));
        let tx_progress = Mutex::new(tx.clone());
        let repaint = egui_ctx.clone();
        let cb: Arc<dyn Fn(f32, &str) + Send + Sync> = Arc::new(move |p, l| {
            if let Ok(tx) = tx_progress.lock() {
                let _ = tx.send(Msg::Progress(p, l.to_string()));
            }
            repaint.request_repaint();
        });
        let core_ctx = Ctx::new(cancel.clone(), cb);
        let repaint = egui_ctx.clone();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(AssertUnwindSafe(|| f(core_ctx)));
            let res = match r {
                Ok(Ok(v)) => Ok(v),
                Ok(Err(e)) => Err(e.to_string()),
                Err(_) => Err("Something went wrong while processing this file.".to_string()),
            };
            let _ = tx.send(Msg::Done(res));
            repaint.request_repaint();
        });
        Self { rx, cancel, progress: 0.0, label: String::new() }
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Call every frame. Returns the result once the task has finished.
    pub fn poll(&mut self) -> Option<Result<T, String>> {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Progress(p, l) => {
                    self.progress = p;
                    if !l.is_empty() {
                        self.label = l;
                    }
                }
                Msg::Done(r) => return Some(r),
            }
        }
        None
    }
}
