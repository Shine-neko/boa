//! Interrupting the evaluation of a [`Context`][crate::Context] from another
//! thread, to run host code on the thread of the context.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A handle to request an interrupt of a [`Context`][crate::Context]'s
/// evaluation, from any thread.
///
/// After [`InterruptHandle::request`], the context calls
/// [`HostHooks::interrupt`][crate::context::HostHooks::interrupt] on its own
/// thread, at the next loop iteration, call of a JavaScript function or return
/// from one. The
/// hook then decides whether the evaluation goes on: a timeout stops it, a
/// sampling profiler records the stack ([`Context::stack_trace`]) and lets it
/// go on.
///
/// Requests made while no JavaScript runs are handled when it next runs.
/// Several requests before the hook runs make one call.
///
/// [`Context::stack_trace`]: crate::Context::stack_trace
#[derive(Debug, Clone, Default)]
pub struct InterruptHandle {
    requested: Arc<AtomicBool>,
}

impl InterruptHandle {
    /// Requests an interrupt.
    pub fn request(&self) {
        self.requested.store(true, Ordering::Release);
    }

    /// Whether an interrupt was requested: a relaxed load, cheap enough to be
    /// checked at every loop iteration and call.
    #[inline]
    pub(crate) fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }

    /// Clears the request, before the hook runs: a request made while it runs
    /// is kept for the next check.
    pub(crate) fn clear(&self) {
        self.requested.store(false, Ordering::Release);
    }
}
