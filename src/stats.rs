// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use crate::json;

const BUDGETS_US: [u64; 3] = [4_000, 8_000, 16_000];

#[derive(Default)]
pub struct Timing {
    count: u64,
    total_us: u64,
    max_us: u64,
    last_us: u64,
    over: [u64; 3],
}

impl Timing {
    pub fn record(&mut self, elapsed: Duration) {
        let micros = u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX);
        self.count += 1;
        self.total_us = self.total_us.saturating_add(micros);
        self.max_us = self.max_us.max(micros);
        self.last_us = micros;
        for (index, budget) in BUDGETS_US.iter().enumerate() {
            if micros > *budget {
                self.over[index] += 1;
            }
        }
    }

    pub fn reset(&mut self) {
        *self = Timing::default();
    }

    pub fn json(&self) -> String {
        let average = self.total_us.checked_div(self.count).unwrap_or(0);
        json::object(&[
            ("count", self.count.to_string()),
            ("average_us", average.to_string()),
            ("max_us", self.max_us.to_string()),
            ("last_us", self.last_us.to_string()),
            ("over_4ms", self.over[0].to_string()),
            ("over_8ms", self.over[1].to_string()),
            ("over_16ms", self.over[2].to_string()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timings_track_the_average_the_maximum_and_the_slow_frames() {
        let mut timing = Timing::default();
        for millis in [1, 3, 9, 20] {
            timing.record(Duration::from_millis(millis));
        }
        let text = timing.json();
        assert!(text.contains("\"count\":4"), "{text}");
        assert!(text.contains("\"average_us\":8250"), "{text}");
        assert!(text.contains("\"max_us\":20000"), "{text}");
        assert!(text.contains("\"last_us\":20000"), "{text}");
        assert!(text.contains("\"over_4ms\":2"), "{text}");
        assert!(text.contains("\"over_8ms\":2"), "{text}");
        assert!(text.contains("\"over_16ms\":1"), "{text}");
    }

    #[test]
    fn an_empty_timing_reports_zeros_and_reset_clears_it() {
        let mut timing = Timing::default();
        assert!(timing.json().contains("\"count\":0"));
        timing.record(Duration::from_millis(5));
        timing.reset();
        assert!(timing.json().contains("\"count\":0"));
    }
}
