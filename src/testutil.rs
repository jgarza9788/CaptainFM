//! Helpers for tests that touch process-wide state (environment variables, the journal, the store).

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// A private sandbox: HOME and the XDG directories point into a fresh temp folder for as long as
/// the value lives. Holding it also serialises tests that depend on global state.
pub struct Sandbox {
    pub root: PathBuf,
    _guard: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

const VARS: [&str; 5] = [
    "HOME",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
];

impl Sandbox {
    pub fn new(name: &str) -> Sandbox {
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = std::env::temp_dir().join(format!("cfm-sandbox-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let saved = VARS.iter().map(|v| (*v, std::env::var_os(v))).collect();
        // SAFETY: tests that change the environment hold LOCK, so no other test thread does so concurrently.
        unsafe {
            std::env::set_var("HOME", &root);
            std::env::set_var("XDG_CONFIG_HOME", root.join(".config"));
            std::env::set_var("XDG_DATA_HOME", root.join(".local/share"));
            std::env::set_var("XDG_CACHE_HOME", root.join(".cache"));
            std::env::set_var("XDG_STATE_HOME", root.join(".local/state"));
        }
        crate::state::reset_store();
        Sandbox {
            root,
            _guard: guard,
            saved,
        }
    }

    /// Create `rel` (a file, or a folder when it ends with `/`) under the sandbox with some content.
    pub fn touch(&self, rel: &str, content: &str) -> PathBuf {
        let path = self.root.join(rel.trim_end_matches('/'));
        if rel.ends_with('/') {
            std::fs::create_dir_all(&path).unwrap();
        } else {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, content).unwrap();
        }
        path
    }

    pub fn path(&self, rel: &str) -> String {
        self.root.join(rel).to_string_lossy().into_owned()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // SAFETY: see `new`; the guard is still held while the variables are restored.
        unsafe {
            for (var, value) in &self.saved {
                match value {
                    Some(v) => std::env::set_var(var, v),
                    None => std::env::remove_var(var),
                }
            }
        }
        crate::state::reset_store();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
