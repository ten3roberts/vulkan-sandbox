use std::time::{Duration, Instant};

/// Measures high precision time
pub struct Clock {
    start: Instant,
}

impl Clock {
    // Creates and starts a new clock
    pub fn new() -> Self {
        Clock {
            start: Instant::now(),
        }
    }

    // Returns the elapsed time
    pub fn elapsed(&self) -> Duration {
        return Instant::now() - self.start;
    }

    // Resets the clock and returns the elapsed time
    pub fn reset(&mut self) -> Duration {
        let elapsed = self.elapsed();

        self.start = Instant::now();
        return elapsed;
    }
}

/// Easier function names for usage of duration
pub trait EasyDuration {
    fn secs(&self) -> f32;
    fn ms(&self) -> u128;
    fn us(&self) -> u128;
    fn ns(&self) -> u128;
}

impl EasyDuration for Duration {
    fn secs(&self) -> f32 {
        self.as_secs_f32()
    }

    fn ms(&self) -> u128 {
        self.as_millis()
    }

    fn us(&self) -> u128 {
        self.as_micros()
    }

    fn ns(&self) -> u128 {
        self.as_nanos()
    }
}
