//! The points between two file-system calls of a walk that reads or writes through directory
//! handles (`contained.rs`, `directory.rs`, `atomic_write.rs`; issues #758 and #771), where a
//! unit test's adversary acts. Outside the tests [`step`] does nothing.
//!
//! A test installs an adversary for its own thread with [`with_adversary`]; every [`step`] that
//! thread passes then calls it, so a test can count a walk's points and replay every schedule of
//! swaps over them (`contained_tests.rs`, `atomic_write_tests.rs`).

/// A point between two file-system calls: the running test's adversary acts here.
#[cfg(not(test))]
pub(crate) fn step() {}

#[cfg(test)]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "macos")),
    allow(
        unused_imports,
        reason = "the adversary tests run where an atomic exchange exists"
    )
)]
pub(crate) use adversary::{step, with_adversary};

#[cfg(test)]
mod adversary {
    use std::cell::RefCell;

    thread_local! {
        /// The adversary of the running test: called at every [`step`].
        static ADVERSARY: RefCell<Option<Box<dyn FnMut()>>> = RefCell::new(None);
    }

    /// A point between two file-system calls: the running test's adversary acts here.
    pub(crate) fn step() {
        ADVERSARY.with(|adversary| {
            if let Some(act) = adversary.borrow_mut().as_mut() {
                act();
            }
        });
    }

    /// Install `act` as this thread's adversary (`None` removes it).
    #[cfg_attr(
        not(any(target_os = "linux", target_os = "macos")),
        allow(
            dead_code,
            reason = "the adversary tests run where an atomic exchange exists"
        )
    )]
    pub(crate) fn with_adversary(act: Option<Box<dyn FnMut()>>) {
        ADVERSARY.with(|adversary| *adversary.borrow_mut() = act);
    }
}
