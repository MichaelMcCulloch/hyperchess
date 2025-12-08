use crate::domain::services::Clock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct SystemClock;

impl SystemClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
    }
}

pub struct FakeClock {
    current_time: Duration,
}

impl FakeClock {
    pub fn new(start_time: Duration) -> Self {
        Self {
            current_time: start_time,
        }
    }

    pub fn advance(&mut self, amount: Duration) {
        self.current_time += amount;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Duration {
        self.current_time
    }
}
