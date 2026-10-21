//! Debounced filesystem watcher that ignores access events.
//!
//! Reading a directory raises inotify access/open events on it; reacting to those would make
//! every refresh trigger the next one, so only real changes are forwarded.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Keeps the underlying watcher alive; dropping it stops watching.
pub struct Watch {
    _watcher: RecommendedWatcher,
}

/// Call `on_change` (on a helper thread) once per burst of changes under `path`.
pub fn watch(
    path: &Path,
    recursive: bool,
    debounce: Duration,
    on_change: impl Fn() + Send + Sync + 'static,
) -> Option<Watch> {
    watch_many(&[path], recursive, debounce, on_change)
}

/// Like [`watch`] for several paths at once (the first one must be watchable).
pub fn watch_many<P: AsRef<Path>>(
    paths: &[P],
    recursive: bool,
    debounce: Duration,
    on_change: impl Fn() + Send + Sync + 'static,
) -> Option<Watch> {
    let on_change = Arc::new(on_change);
    let pending = Arc::new(AtomicBool::new(false));

    let mut watcher =
        notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            if pending.swap(true, Ordering::SeqCst) {
                return;
            }
            let pending = Arc::clone(&pending);
            let on_change = Arc::clone(&on_change);
            std::thread::spawn(move || {
                std::thread::sleep(debounce);
                pending.store(false, Ordering::SeqCst);
                on_change();
            });
        })
        .ok()?;

    let mode = if recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    for (i, path) in paths.iter().enumerate() {
        let watched = watcher.watch(path.as_ref(), mode);
        // A vanished sub-folder shouldn't stop the others from being watched.
        if i == 0 {
            watched.ok()?;
        }
    }
    Some(Watch { _watcher: watcher })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    fn wait_for(count: &AtomicUsize, at_least: usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if count.load(Ordering::SeqCst) >= at_least {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("cfm-watch-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn creating_a_file_fires_once_per_burst() {
        let dir = temp("create");
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        let _w = watch(&dir, false, Duration::from_millis(50), move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watcher");
        for i in 0..5 {
            std::fs::write(dir.join(format!("f{i}")), "x").unwrap();
        }
        assert!(wait_for(&count, 1));
        std::thread::sleep(Duration::from_millis(300));
        assert!(count.load(Ordering::SeqCst) <= 2, "a burst should be debounced");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reading_the_folder_does_not_fire() {
        let dir = temp("read");
        std::fs::write(dir.join("a"), "x").unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let c = Arc::clone(&count);
        let _w = watch(&dir, false, Duration::from_millis(50), move || {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watcher");
        for _ in 0..5 {
            let _ = std::fs::read_dir(&dir).unwrap().count();
            let _ = std::fs::read(dir.join("a"));
        }
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(count.load(Ordering::SeqCst), 0, "access events must be ignored");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn watching_a_missing_folder_fails() {
        assert!(watch(Path::new("/definitely/not/here"), false, Duration::from_millis(10), || {}).is_none());
    }
}
