use std::cell::RefCell;
use std::time::Instant;

thread_local! {
    static ENABLED: RefCell<bool>        = RefCell::new(false);
    static LOG:     RefCell<Vec<String>> = RefCell::new(Vec::new());
    static T0:      RefCell<Option<Instant>> = RefCell::new(None);
}

pub fn enable() {
    ENABLED.with(|e| *e.borrow_mut() = true);
    T0.with(|t|  *t.borrow_mut() = Some(Instant::now()));
}

pub fn is_enabled() -> bool {
    ENABLED.with(|e| *e.borrow())
}

pub fn log(msg: impl Into<String>) {
    if !is_enabled() { return; }
    let ms = T0.with(|t| t.borrow().map(|i| i.elapsed().as_millis()).unwrap_or(0));
    LOG.with(|l| l.borrow_mut().push(format!("{ms:>6}ms  {}", msg.into())));
}

/// Drain and return all collected entries (empties the internal buffer).
pub fn take() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}
