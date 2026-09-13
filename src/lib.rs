#![doc = include_str!("../README.md")]
//! Measured solar response and live radiation forecasts.
pub mod weather;
use crate::measurements::HALF_HOUR_SLOTS;
use crate::measurements::{live_slot, local_slot, prepare_history};
pub use crate::measurements::{Observation, Reading};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use std::ops::Range;
pub use weather::{Weather, WeatherRun};
mod model;
pub trait RadiationForecast {
    fn predicted(&self, time: DateTime<Utc>, known_at: DateTime<Utc>) -> Option<f64>;
}

/// Predict generated energy for each complete half-hour using causal roof calibration.
pub fn predict(
    history: impl IntoIterator<Item = Observation>,
    horizon: Range<DateTime<Utc>>,
    now: DateTime<Utc>,
    timezone: Tz,
    weather: Option<&dyn RadiationForecast>,
    reading: Option<Reading>,
) -> Vec<f64> {
    let history = prepare_history(history, now);
    let solar_start = model::history_start(&history, now, timezone, weather);
    let mut buckets: [Vec<Observation>; HALF_HOUR_SLOTS] = std::array::from_fn(|_| Vec::new());
    for point in &history {
        let slot = local_slot(point.time, timezone);
        if solar_start.is_none_or(|start| point.time >= start) {
            for neighbour in [(slot + 47) % 48, slot, (slot + 1) % 48] {
                buckets[neighbour].push(*point);
            }
        }
    }
    let count = ((horizon.end - horizon.start).num_seconds() / 1800).max(0) as usize;
    let mut result = vec![0.0; count];
    for (index, value) in result.iter_mut().enumerate() {
        let time = horizon.start + Duration::minutes(index as i64 * 30);
        if let Some(predicted) = model::predict(
            &buckets[local_slot(time, timezone)],
            time,
            now,
            timezone,
            weather,
        ) {
            *value = predicted;
        }
    }
    if let Some(reading) = reading {
        if let Some(index) = live_slot(reading, &horizon, now) {
            result[index] = reading.power_kw * 0.5;
        }
    }
    result
}

mod history;
mod measurements;
pub use history::History;

mod config;
pub use config::WeatherConfig;
