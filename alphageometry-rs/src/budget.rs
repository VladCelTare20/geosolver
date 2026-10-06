use std::cell::Cell;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::time::Instant;

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

struct Expired;

/// Unwind out of the enclosing [`within`] once its deadline has passed. A
/// no-op on a thread outside `within`.
pub fn check() {
    if DEADLINE.with(Cell::get).is_some_and(|d| Instant::now() >= d) {
        std::panic::panic_any(Expired);
    }
}

/// Run `f` on this thread with `deadline`: `None` when `f` was stopped by a
/// [`check`] past it. Other panics propagate unchanged.
pub fn within<R>(deadline: Option<Instant>, f: impl FnOnce() -> R) -> Option<R> {
    let prev = DEADLINE.with(|d| d.replace(deadline));
    let out = catch_unwind(AssertUnwindSafe(|| crate::quiet_panic::quiet(f)));
    DEADLINE.with(|d| d.set(prev));
    match out {
        Ok(r) => Some(r),
        Err(e) if e.is::<Expired>() => None,
        Err(e) => resume_unwind(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_check_past_the_deadline_stops_the_work_and_nothing_else_does() {
        let d = Instant::now() + Duration::from_millis(30);
        let mut spins = 0u64;
        let r = within(Some(d), || loop {
            spins += 1;
            check();
        });
        assert!(r.is_none());
        assert!(spins > 0);
        assert!(Instant::now() >= d);
        assert_eq!(within(None, || 7), Some(7));
        check();
        let other = std::panic::catch_unwind(|| within(Some(d + Duration::from_secs(60)), || panic!("boom")));
        assert!(other.is_err());
        check();
    }
}
