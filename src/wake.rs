//! Keeps the computer from locking the screen or sleeping while the program is open: somebody
//! practising on a drum kit touches neither the keyboard nor the mouse, and the system takes the
//! quiet for leaving. Windows asks for it with `SetThreadExecutionState`; other systems do nothing
//! yet (Linux and macOS follow when they can be tried).
//!
//! This is the only module with `unsafe` code: one call that takes flags and no pointers. The
//! crate denies `unsafe_code` everywhere else (`Cargo.toml`).

/// What the system is asked to do.
pub trait Backend {
    /// Asks the system to keep the display and the computer awake. `false` if it refused.
    fn hold(&mut self) -> bool;
    /// Lets the system go back to its own timers.
    fn release(&mut self);
}

/// The wish to keep the screen awake, and whether the system has been asked: the system is only
/// told when the wish changes, so this can be called on every frame.
pub struct Wake {
    backend: Box<dyn Backend>,
    held: bool,
}

impl Wake {
    /// With the system's own way of doing it.
    pub fn new() -> Wake {
        Wake::with(Box::new(system::Native))
    }

    pub fn with(backend: Box<dyn Backend>) -> Wake {
        Wake { backend, held: false }
    }

    /// Whether the system is being kept awake.
    #[cfg(test)]
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Makes it as wanted. A refusal of the system is tried again on the next call.
    pub fn sync(&mut self, wanted: bool) {
        if wanted && !self.held {
            self.held = self.backend.hold();
        } else if !wanted && self.held {
            self.backend.release();
            self.held = false;
        }
    }

    /// Whether this system can be kept awake at all (the switch is shown as unavailable if not).
    pub const fn available() -> bool {
        cfg!(windows)
    }
}

impl Drop for Wake {
    fn drop(&mut self) {
        self.sync(false);
    }
}

#[cfg(windows)]
mod system {
    use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};

    /// The state belongs to the thread that sets it, which is the window's thread: `Wake` is used
    /// from there only.
    pub struct Native;

    #[allow(unsafe_code)]
    fn set(flags: u32) -> bool {
        // SAFETY: the function takes flags only and reads and writes no memory of ours.
        unsafe { SetThreadExecutionState(flags) != 0 }
    }

    impl super::Backend for Native {
        fn hold(&mut self) -> bool {
            set(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED)
        }

        fn release(&mut self) {
            set(ES_CONTINUOUS);
        }
    }
}

#[cfg(not(windows))]
mod system {
    /// Nothing is done on this system yet.
    pub struct Native;

    impl super::Backend for Native {
        fn hold(&mut self) -> bool {
            false
        }

        fn release(&mut self) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct Calls {
        holds: u32,
        releases: u32,
        refuse: bool,
    }

    struct Fake(Rc<RefCell<Calls>>);

    impl Backend for Fake {
        fn hold(&mut self) -> bool {
            let mut c = self.0.borrow_mut();
            c.holds += 1;
            !c.refuse
        }

        fn release(&mut self) {
            self.0.borrow_mut().releases += 1;
        }
    }

    fn wake() -> (Wake, Rc<RefCell<Calls>>) {
        let calls = Rc::new(RefCell::new(Calls::default()));
        (Wake::with(Box::new(Fake(calls.clone()))), calls)
    }

    #[test]
    fn the_system_is_asked_once_however_often_it_is_synced() {
        let (mut w, calls) = wake();
        for _ in 0..100 {
            w.sync(true);
        }
        assert!(w.is_held());
        assert_eq!((calls.borrow().holds, calls.borrow().releases), (1, 0));
        for _ in 0..100 {
            w.sync(false);
        }
        assert!(!w.is_held());
        assert_eq!((calls.borrow().holds, calls.borrow().releases), (1, 1));
        w.sync(false);
        assert_eq!(calls.borrow().releases, 1, "nothing to release");
    }

    #[test]
    fn a_refusal_is_tried_again_and_nothing_is_released_that_was_not_held() {
        let (mut w, calls) = wake();
        calls.borrow_mut().refuse = true;
        w.sync(true);
        w.sync(true);
        assert!(!w.is_held());
        assert_eq!(calls.borrow().holds, 2, "asked again on the next call");
        calls.borrow_mut().refuse = false;
        w.sync(true);
        assert!(w.is_held());
        drop(w);
        assert_eq!(calls.borrow().releases, 1);
    }

    #[test]
    fn it_lets_go_when_it_is_dropped_while_held() {
        let (mut w, calls) = wake();
        w.sync(true);
        drop(w);
        assert_eq!(calls.borrow().releases, 1);
        let (w2, calls2) = wake();
        drop(w2);
        assert_eq!(calls2.borrow().releases, 0, "never held, nothing to release");
    }

    #[test]
    fn the_system_backend_is_there_and_is_only_available_where_it_works() {
        assert_eq!(Wake::available(), cfg!(windows));
        // On a system where it works, asking and letting go leave nothing held.
        let mut w = Wake::new();
        w.sync(true);
        assert_eq!(w.is_held(), Wake::available());
        w.sync(false);
        assert!(!w.is_held());
    }
}
