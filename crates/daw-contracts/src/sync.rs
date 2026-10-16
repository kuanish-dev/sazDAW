//! Small shared helpers for control threads (never the audio callback).

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks the mutex even if another thread panicked while holding it: the panic is already logged,
/// and carrying on with the last state is better than bringing the whole app down.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
