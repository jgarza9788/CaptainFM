//! Locking that survives a poisoned lock.
//!
//! A `Mutex` or `RwLock` is "poisoned" when a thread panicked while holding it. `.lock().unwrap()` then panics in
//! every other thread that touches the lock, so one bug in a worker takes the whole app down. The data behind our
//! locks is plain state (queues, caches, counters) that stays usable, so these helpers take the guard anyway.
//! (The `dist` profile aborts on panic, so there a lock cannot be poisoned; the `release` and test builds unwind.)

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

pub trait MutexExt<T> {
    /// `lock()` that returns the guard even when another thread panicked while holding it.
    fn locked(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn locked(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub trait RwLockExt<T> {
    fn read_locked(&self) -> RwLockReadGuard<'_, T>;
    fn write_locked(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockExt<T> for RwLock<T> {
    fn read_locked(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|e| e.into_inner())
    }
    fn write_locked(&self) -> RwLockWriteGuard<'_, T> {
        self.write().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_poisoned_mutex_still_locks() {
        let m = Arc::new(Mutex::new(1));
        let m2 = Arc::clone(&m);
        let _ = std::thread::spawn(move || {
            let _g = m2.lock().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(m.is_poisoned());
        *m.locked() += 1;
        assert_eq!(*m.locked(), 2);
    }

    #[test]
    fn a_poisoned_rwlock_still_locks() {
        let l = Arc::new(RwLock::new(vec![1]));
        let l2 = Arc::clone(&l);
        let _ = std::thread::spawn(move || {
            let _g = l2.write().unwrap();
            panic!("poison it");
        })
        .join();
        assert!(l.is_poisoned());
        l.write_locked().push(2);
        assert_eq!(l.read_locked().len(), 2);
    }
}
