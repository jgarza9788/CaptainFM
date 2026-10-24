//! Copy and move jobs the user can watch: progress, speed, pause and cancel.
//!
//! Each paste registers a [`Ctl`] here. The worker thread updates its counters while it copies and the
//! UI polls [`snapshot`]. Cancel and pause are flags the copy loop checks between chunks.

use std::io;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::json;

/// What to do when the destination already has an item of the same name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Keep both: the new one gets a `(copy)` name.
    Both,
    /// Move the old one to the trash, then put the new one in its place.
    Replace,
    /// Leave the old one alone and do not transfer the new one.
    Skip,
}

impl Policy {
    pub fn parse(s: &str) -> Policy {
        match s {
            "replace" => Policy::Replace,
            "skip" => Policy::Skip,
            _ => Policy::Both,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Queued,
    Running,
    Done,
    Cancelled,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Queued => "queued",
            State::Running => "running",
            State::Done => "done",
            State::Cancelled => "cancelled",
        }
    }
}

struct Speed {
    at: Instant,
    bytes: u64,
    per_sec: f64,
}

pub struct Ctl {
    id: i32,
    label: String,
    dest: String,
    pub bytes_done: AtomicU64,
    pub bytes_total: AtomicU64,
    pub items_done: AtomicU64,
    pub items_total: AtomicU64,
    cancel: AtomicBool,
    pause: AtomicBool,
    state: Mutex<State>,
    current: Mutex<String>,
    speed: Mutex<Speed>,
}

static REGISTRY: Mutex<Vec<Arc<Ctl>>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicI32 = AtomicI32::new(1);

impl Ctl {
    fn build(id: i32, label: &str, dest: &str, items: u64) -> Ctl {
        Ctl {
            id,
            label: label.to_owned(),
            dest: dest.to_owned(),
            bytes_done: AtomicU64::new(0),
            bytes_total: AtomicU64::new(0),
            items_done: AtomicU64::new(0),
            items_total: AtomicU64::new(items),
            cancel: AtomicBool::new(false),
            pause: AtomicBool::new(false),
            state: Mutex::new(State::Queued),
            current: Mutex::new(String::new()),
            speed: Mutex::new(Speed { at: Instant::now(), bytes: 0, per_sec: 0.0 }),
        }
    }

    /// A control nobody watches, for undo, redo and trashing, which never pause or cancel.
    pub fn detached() -> Ctl {
        Ctl::build(0, "", "", 0)
    }

    /// Register a queued transfer of `items` top-level items into `dest`.
    pub fn register(label: &str, dest: &str, items: usize) -> Arc<Ctl> {
        let ctl = Arc::new(Ctl::build(NEXT_ID.fetch_add(1, Ordering::Relaxed), label, dest, items as u64));
        let mut reg = REGISTRY.lock().unwrap();
        // Finished jobs stay listed for a while, but not forever.
        let finished = reg.iter().filter(|c| c.state() >= State::Done).count();
        let mut drop_n = finished.saturating_sub(8);
        reg.retain(|c| {
            let old = c.state() >= State::Done && drop_n > 0;
            if old {
                drop_n -= 1;
            }
            !old
        });
        reg.push(ctl.clone());
        ctl
    }

    pub fn state(&self) -> State {
        *self.state.lock().unwrap()
    }

    pub fn set_state(&self, s: State) {
        *self.state.lock().unwrap() = s;
    }

    pub fn set_current(&self, name: &str) {
        *self.current.lock().unwrap() = name.to_owned();
    }

    pub fn add_bytes(&self, n: u64) {
        self.bytes_done.fetch_add(n, Ordering::Relaxed);
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Called between chunks: waits while paused, and fails with `Interrupted` once cancelled.
    pub fn checkpoint(&self) -> io::Result<()> {
        while self.pause.load(Ordering::Relaxed) && !self.cancelled() {
            std::thread::sleep(Duration::from_millis(80));
        }
        if self.cancelled() { Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled")) } else { Ok(()) }
    }

    fn snapshot(&self) -> serde_json::Value {
        let state = self.state();
        let done = self.bytes_done.load(Ordering::Relaxed);
        let total = self.bytes_total.load(Ordering::Relaxed);
        let paused = self.pause.load(Ordering::Relaxed);
        let per_sec = {
            let mut s = self.speed.lock().unwrap();
            let dt = s.at.elapsed().as_secs_f64();
            if state != State::Running || paused {
                s.per_sec = 0.0;
                s.at = Instant::now();
                s.bytes = done;
            } else if dt >= 0.4 {
                let inst = done.saturating_sub(s.bytes) as f64 / dt;
                s.per_sec = if s.per_sec == 0.0 { inst } else { s.per_sec * 0.6 + inst * 0.4 };
                s.at = Instant::now();
                s.bytes = done;
            }
            s.per_sec
        };
        let items_done = self.items_done.load(Ordering::Relaxed);
        let items_total = self.items_total.load(Ordering::Relaxed);
        let fraction = if total > 0 {
            (done as f64 / total as f64).min(1.0)
        } else if items_total > 0 {
            (items_done as f64 / items_total as f64).min(1.0)
        } else {
            0.0
        };
        let eta = if per_sec > 1.0 && total > done { ((total - done) as f64 / per_sec).ceil() as i64 } else { -1 };
        json!({
            "id": self.id,
            "label": self.label,
            "dest": self.dest,
            "state": state.name(),
            "paused": paused,
            "current": *self.current.lock().unwrap(),
            "bytesDone": done,
            "bytesTotal": total,
            "itemsDone": items_done,
            "itemsTotal": items_total,
            "fraction": if state == State::Done { 1.0 } else { fraction },
            "speed": per_sec,
            "eta": eta,
        })
    }
}

impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some((*self as u8).cmp(&(*other as u8)))
    }
}

/// Every transfer, oldest first, as a JSON array.
pub fn snapshot() -> String {
    let reg = REGISTRY.lock().unwrap();
    serde_json::Value::Array(reg.iter().map(|c| c.snapshot()).collect()).to_string()
}

fn find(id: i32) -> Option<Arc<Ctl>> {
    REGISTRY.lock().unwrap().iter().find(|c| c.id == id).cloned()
}

pub fn cancel(id: i32) {
    if let Some(c) = find(id) {
        c.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
pub fn cancel_ctl(c: &Ctl) {
    c.cancel.store(true, Ordering::Relaxed);
}

pub fn pause(id: i32, paused: bool) {
    if let Some(c) = find(id) {
        c.pause.store(paused, Ordering::Relaxed);
    }
}

/// Forget the transfers that are over.
pub fn clear_finished() {
    REGISTRY.lock().unwrap().retain(|c| c.state() < State::Done);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_stops_a_checkpoint_and_even_a_paused_one() {
        let c = Ctl::register("Copied", "/tmp", 1);
        assert!(c.checkpoint().is_ok());
        pause(c.id, true);
        let id = c.id;
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            cancel(id);
        });
        assert_eq!(c.checkpoint().unwrap_err().kind(), io::ErrorKind::Interrupted);
        t.join().unwrap();
    }

    #[test]
    fn snapshot_reports_progress() {
        let c = Ctl::register("Moved", "/tmp", 2);
        c.set_state(State::Running);
        c.bytes_total.store(200, Ordering::Relaxed);
        c.add_bytes(50);
        let v: serde_json::Value = serde_json::from_str(&snapshot()).unwrap();
        let mine = v.as_array().unwrap().iter().find(|e| e["id"] == c.id).unwrap();
        assert_eq!(mine["state"], "running");
        assert_eq!(mine["fraction"], 0.25);
        c.set_state(State::Done);
        clear_finished();
    }
}
