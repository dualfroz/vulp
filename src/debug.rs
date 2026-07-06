//! Timing debug log shown in the hub's bottom debug panel (`--debug` flag).
//! Backed by thread-locals rather than a field threaded through every
//! function, since logging is opt-in and call sites are scattered across
//! the codebase wherever something worth timing happens.

use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static ENABLED: RefCell<bool>        = const { RefCell::new(false) };
    static LOG:     RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static T0:      RefCell<Option<Instant>> = const { RefCell::new(None) };
}

/// Turns on debug logging and starts the elapsed-time clock used to
/// timestamp subsequent `log` entries.
pub fn enable() {
    ENABLED.with(|e| *e.borrow_mut() = true);
    T0.with(|t|  *t.borrow_mut() = Some(Instant::now()));
}

/// Whether debug logging is currently on.
pub fn is_enabled() -> bool {
    ENABLED.with(|e| *e.borrow())
}

/// Appends a timestamped entry to the debug log. A no-op when logging isn't
/// enabled, so call sites don't need to guard every call with `is_enabled()`.
pub fn log(msg: impl Into<String>) {
    if !is_enabled() { return; }
    let ms = T0.with(|t| t.borrow().map(|i| i.elapsed().as_millis()).unwrap_or(0));
    LOG.with(|l| l.borrow_mut().push(format!("{ms:>6}ms  {}", msg.into())));
}

/// Drain and return all collected entries (empties the internal buffer).
pub fn take() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}
