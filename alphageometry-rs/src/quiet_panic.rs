//! Race-free suppression of panic output.
//!
//! The engine deliberately provokes and catches panics (degenerate sampled
//! figures, imprecise candidate constructions). Their default "thread panicked"
//! message is noise, so it is muted — but never by swapping the process-wide
//! hook per call: a `take_hook`/`set_hook` pair races between concurrent
//! callers and is never restored if a panic escapes between the two.
//!
//! Instead one hook is installed exactly once. It stays silent while the
//! panicking thread is inside [`quiet`], or while it is a rayon worker and some
//! thread is inside [`quiet`] (the aux search fans candidate runs out to the
//! pool); every other panic goes to the previously installed hook unchanged.
//! `DDAR_DEBUG_PANICS=1` prints suppressed panics to stderr.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Once;

static INSTALL: Once = Once::new();
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn install() {
    INSTALL.call_once(|| {
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let here = DEPTH.with(|d| d.get()) > 0;
            let pool = rayon::current_thread_index().is_some() && ACTIVE.load(Ordering::SeqCst) > 0;
            if here || pool {
                if std::env::var_os("DDAR_DEBUG_PANICS").is_some_and(|v| !v.is_empty()) {
                    eprintln!("[ddar panic] {info}");
                }
            } else {
                prev(info);
            }
        }));
    });
}

struct Guard;

impl Drop for Guard {
    fn drop(&mut self) {
        DEPTH.with(|d| d.set(d.get() - 1));
        ACTIVE.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Run `f` with panic output muted (see the module docs). The mute is lifted
/// when `f` returns or unwinds; it does not catch the panic itself.
pub fn quiet<R>(f: impl FnOnce() -> R) -> R {
    install();
    DEPTH.with(|d| d.set(d.get() + 1));
    ACTIVE.fetch_add(1, Ordering::SeqCst);
    let _guard = Guard;
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_is_lifted_when_a_panic_escapes() {
        let r = std::panic::catch_unwind(|| quiet(|| panic!("escapes")));
        assert!(r.is_err());
        assert_eq!(DEPTH.with(|d| d.get()), 0);
    }

    #[test]
    fn mute_is_per_thread() {
        let other = std::thread::spawn(|| DEPTH.with(|d| d.get()));
        quiet(|| {
            assert_eq!(DEPTH.with(|d| d.get()), 1);
        });
        assert_eq!(other.join().unwrap(), 0);
    }
}
