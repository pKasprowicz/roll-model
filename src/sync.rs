//! One policy for the plain queues that threads share (MIDI events, error messages): a lock whose
//! holder panicked is still locked and usable, because a queue of plain values cannot be left half
//! changed. It is recovered, never turned into an unrelated error and never ignored.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, also when a thread panicked while it held it.
pub fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn a_lock_whose_holder_panicked_is_still_usable() {
        let queue = Arc::new(Mutex::new(vec![1, 2]));
        let held = Arc::clone(&queue);
        let result = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("a thread dies holding the lock");
        })
        .join();
        assert!(result.is_err());
        assert!(queue.lock().is_err(), "the lock is poisoned");
        locked(&queue).push(3);
        assert_eq!(*locked(&queue), [1, 2, 3], "the data is there and can still be used");
    }
}
