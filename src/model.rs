//! Learn a roof's response to sunlight without amplifying dark-hour noise.

use super::{local_slot, Observation, RadiationForecast};
use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use std::collections::BTreeMap;

pub(crate) const HISTORY_DAYS: i64 = 14;
const MIN_CALIBRATION_RADIATION: f64 = 50.0;
const MIN_EXACT_SAMPLES: usize = 3;
const MIN_ESTABLISHED_SOLAR_KWH: f64 = 0.01;

struct DayEvidence {
    first_at: DateTime<Utc>,
    observed_slots: u64,
    producing_slots: u64,
    calibrated_producing_slots: u64,
    any_output: bool,
}

/// Detect a newly reporting solar source once per plan, before bucketing history.
/// Several established production days prevent old zero readings from holding a
/// commissioned or repaired source at zero for most of the calibration window.
pub(crate) fn history_start(
    history: &[Observation],
    now: DateTime<Utc>,
    timezone: Tz,
    weather: Option<&dyn RadiationForecast>,
) -> Option<DateTime<Utc>> {
    weather?;
    let mut days = BTreeMap::new();
    for point in history.iter().filter(|point| eligible(point, now)) {
        let date = point.time.with_timezone(&timezone).date_naive();
        let slot = 1 << local_slot(point.time, timezone);
        let day = days.entry(date).or_insert(DayEvidence {
            first_at: point.time,
            observed_slots: 0,
            producing_slots: 0,
            calibrated_producing_slots: 0,
            any_output: false,
        });
        day.first_at = day.first_at.min(point.time);
        day.observed_slots |= slot;
        day.any_output |= point.energy_kwh > 0.0;
        if point.energy_kwh >= MIN_ESTABLISHED_SOLAR_KWH {
            day.producing_slots |= slot;
            if radiation(weather, point.time, point.time)
                .is_some_and(|value| value >= MIN_CALIBRATION_RADIATION)
            {
                day.calibrated_producing_slots |= slot;
            }
        }
    }
    // Three distinct half-hours span at least one hour. A repeated single-slot
    // spike is insufficient; previously established output also blocks a reset.
    let (&first_day, first_evidence) = days
        .iter()
        .find(|(_, day)| day.producing_slots.count_ones() >= 3)?;
    let producing_days: Vec<_> = days
        .range(first_day..)
        .filter(|(_, day)| day.calibrated_producing_slots.count_ones() >= 3)
        .collect();
    if producing_days.len() < 2
        || *producing_days.last()?.0 < now.with_timezone(&timezone).date_naive() - Duration::days(3)
    {
        return None;
    }
    let daylight_slots = producing_days
        .iter()
        .fold(0, |slots, (_, day)| slots | day.calibrated_producing_slots);
    // Check coverage at the newly confirmed daylight clock times. Earlier
    // weather archives may not exist before a telemetry source was repaired.
    let previous_zero_days = days
        .range(..first_day)
        .filter(|(_, day)| {
            !day.any_output && (day.observed_slots & daylight_slots).count_ones() >= 3
        })
        .count();
    (previous_zero_days >= 2).then_some(first_evidence.first_at)
}

fn eligible(point: &Observation, now: DateTime<Utc>) -> bool {
    point.time >= now - Duration::days(HISTORY_DAYS)
        && point.time + Duration::minutes(30) <= now
        && point.energy_kwh.is_finite()
        && point.energy_kwh >= 0.0
}

fn radiation(
    weather: Option<&dyn RadiationForecast>,
    time: DateTime<Utc>,
    known_at: DateTime<Utc>,
) -> Option<f64> {
    weather?
        .predicted(time, known_at)
        .filter(|value| value.is_finite() && (0.0..=2000.0).contains(value))
}

/// A weighted median keeps an isolated telemetry or weather error from changing
/// the learned roof response. Equal middle weights use the mean of both values.
fn median(samples: &mut [(f64, f64)]) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    samples.sort_by(|left, right| left.0.total_cmp(&right.0));
    let middle = samples.iter().map(|sample| sample.1).sum::<f64>() / 2.0;
    let mut cumulative = 0.0;
    for (index, &(value, weight)) in samples.iter().enumerate() {
        cumulative += weight;
        if cumulative > middle {
            return Some(value);
        }
        if cumulative == middle {
            return Some((value + samples[index + 1].0) / 2.0);
        }
    }
    None
}

pub(crate) fn predict(
    history: &[Observation],
    target: DateTime<Utc>,
    now: DateTime<Utc>,
    timezone: Tz,
    weather: Option<&dyn RadiationForecast>,
) -> Option<f64> {
    let predicted_radiation = radiation(weather, target, now);
    if predicted_radiation == Some(0.0) {
        return Some(0.0);
    }

    let target_slot = local_slot(target, timezone);
    let mut observed = Vec::new();
    let mut exact = Vec::new();
    let mut nearby = Vec::new();
    for point in history.iter().filter(|point| eligible(point, now)) {
        let distance = target_slot.abs_diff(local_slot(point.time, timezone));
        let distance = distance.min(48 - distance);
        if distance > 1 {
            continue;
        }
        if distance == 0 {
            observed.push((point.energy_kwh, 1.0));
        }
        // Freeze calibration at what was known when this historical slot began.
        // In particular, later forecast revisions cannot rewrite the training data.
        let Some(historical_radiation) = radiation(weather, point.time, point.time)
            .filter(|value| *value >= MIN_CALIBRATION_RADIATION)
        else {
            continue;
        };
        let response = point.energy_kwh / historical_radiation;
        if distance == 0 {
            exact.push((response, 1.0));
        } else {
            nearby.push((response, 0.25));
        }
    }

    if let Some(predicted_radiation) = predicted_radiation {
        if exact.len() < MIN_EXACT_SAMPLES && nearby.len() >= 2 {
            // Borrow only adjacent half-hours. Their combined influence cannot
            // outweigh an observed response at this clock time; whole-day pooling
            // would lose the roof's orientation and recurring shading pattern.
            if !exact.is_empty() {
                let nearby_weight = (exact.len() as f64 * 0.5 / nearby.len() as f64).min(0.25);
                for sample in &mut nearby {
                    sample.1 = nearby_weight;
                }
            }
            exact.extend(nearby);
        }
        if let Some(response) = median(&mut exact) {
            // A sufficiently lit calibration supports a brighter forecast without
            // the old 3x cap, which systematically missed sunshine after dull days.
            return Some(response * predicted_radiation);
        }
    }
    median(&mut observed)
}

#[cfg(test)]
#[path = "../tests/unit/forecast.rs"]
mod tests;
