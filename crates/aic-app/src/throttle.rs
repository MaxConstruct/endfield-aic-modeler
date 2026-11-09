//! A throttle that fires on both edges, like VueUse's `useThrottleFn`: the first call after a quiet
//! spell runs at once, later ones at most once per interval, and the last one always runs.

use std::time::{Duration, Instant};

/// What to do about work that is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Poll {
    /// Nothing is waiting.
    Idle,
    /// Do it now.
    Run,
    /// Do it at this time, if it is still waiting then.
    Wait(Instant),
}

#[derive(Debug, Clone)]
pub struct Throttle {
    interval: Duration,
    last_run: Option<Instant>,
}

impl Throttle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_run: None,
        }
    }

    /// Asks whether waiting work may run at `now`. A `Run` answer counts as running it.
    pub fn poll(&mut self, now: Instant, waiting: bool) -> Poll {
        if !waiting {
            return Poll::Idle;
        }
        match self.last_run {
            Some(last) if now < last + self.interval => Poll::Wait(last + self.interval),
            _ => {
                self.last_run = Some(now);
                Poll::Run
            }
        }
    }

    /// Records a run that happened outside the schedule, such as a save before quitting.
    pub fn ran(&mut self, now: Instant) {
        self.last_run = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIVE: Duration = Duration::from_secs(5);

    #[test]
    fn the_first_change_runs_at_once() {
        let start = Instant::now();
        let mut throttle = Throttle::new(FIVE);
        assert_eq!(throttle.poll(start, false), Poll::Idle);
        assert_eq!(throttle.poll(start, true), Poll::Run);
    }

    #[test]
    fn changes_inside_the_interval_wait_for_its_end_then_run_once() {
        let start = Instant::now();
        let mut throttle = Throttle::new(FIVE);
        throttle.poll(start, true);
        let soon = start + Duration::from_secs(1);
        assert_eq!(throttle.poll(soon, true), Poll::Wait(start + FIVE));
        assert_eq!(
            throttle.poll(start + Duration::from_secs(3), true),
            Poll::Wait(start + FIVE)
        );
        assert_eq!(throttle.poll(start + FIVE, true), Poll::Run);
        assert_eq!(throttle.poll(start + FIVE, false), Poll::Idle);
    }

    #[test]
    fn a_change_after_a_quiet_spell_runs_at_once_again() {
        let start = Instant::now();
        let mut throttle = Throttle::new(FIVE);
        throttle.poll(start, true);
        assert_eq!(
            throttle.poll(start + Duration::from_secs(60), true),
            Poll::Run
        );
    }
}
