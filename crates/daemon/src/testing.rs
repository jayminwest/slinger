//! Helpers the tests share.

use std::time::{Duration, Instant};

/// Calls `take` until it does not fail with a lock that is `taken`, for ten
/// seconds at most. A lock just released can stay held for a moment: a
/// process another test spawns holds a copy of every open file until its exec.
pub(crate) fn when_free<T, E>(
    mut take: impl FnMut() -> Result<T, E>,
    taken: impl Fn(&E) -> bool,
) -> Result<T, E> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match take() {
            Err(e) if taken(&e) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => return result,
        }
    }
}
