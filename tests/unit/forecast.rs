use super::*;
use chrono::TimeZone;

struct Weather(Vec<(DateTime<Utc>, f64)>);

impl RadiationForecast for Weather {
    fn predicted(&self, time: DateTime<Utc>, _known_at: DateTime<Utc>) -> Option<f64> {
        self.0.iter().find(|row| row.0 == time).map(|row| row.1)
    }
}

fn target() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 5, 12, 0, 0).unwrap()
}

fn observation(time: DateTime<Utc>, energy_kwh: f64) -> Observation {
    Observation { time, energy_kwh }
}

fn forecast(history: &[Observation], weather: Option<&dyn RadiationForecast>) -> Option<f64> {
    predict(history, target(), target(), chrono_tz::UTC, weather)
}

#[test]
fn bright_weather_can_exceed_three_times_a_dull_days_generation() {
    let previous = target() - Duration::days(1);
    let weather = Weather(vec![(previous, 100.0), (target(), 800.0)]);
    assert_eq!(
        forecast(&[observation(previous, 0.1)], Some(&weather)),
        Some(0.8)
    );
}

#[test]
fn a_dark_hour_and_an_isolated_output_spike_do_not_distort_roof_calibration() {
    let mut history = Vec::new();
    let mut weather = Weather(vec![(target(), 800.0)]);
    for (days_ago, radiation, solar) in [
        (1, 1.0, 0.2),
        (2, 100.0, 0.1),
        (3, 200.0, 0.2),
        (4, 100.0, 20.0),
    ] {
        let time = target() - Duration::days(days_ago);
        history.push(observation(time, solar));
        weather.0.push((time, radiation));
    }
    assert_eq!(forecast(&history, Some(&weather)), Some(0.8));
}

#[test]
fn missing_weather_falls_back_to_robust_same_clock_history() {
    let history = [0.2, 0.2, 20.0]
        .into_iter()
        .enumerate()
        .map(|(day, solar)| observation(target() - Duration::days(day as i64 + 1), solar))
        .collect::<Vec<_>>();
    assert_eq!(forecast(&history, None), Some(0.2));
    let weather = Weather(vec![(target(), 800.0)]);
    assert_eq!(forecast(&history, Some(&weather)), Some(0.2));
    let invalid_weather = Weather(vec![(target(), f64::NAN)]);
    assert_eq!(forecast(&history, Some(&invalid_weather)), Some(0.2));
}

#[test]
fn adjacent_half_hours_fill_a_gap_without_borrowing_another_roof_orientation() {
    let previous = target() - Duration::days(1);
    let before = previous - Duration::minutes(30);
    let after = previous + Duration::minutes(30);
    let far = previous + Duration::hours(3);
    let weather = Weather(vec![
        (before, 200.0),
        (after, 100.0),
        (far, 100.0),
        (target(), 800.0),
    ]);
    let history = [
        observation(before, 0.2),
        observation(after, 0.1),
        observation(far, 10.0),
    ];
    assert_eq!(forecast(&history, Some(&weather)), Some(0.8));
    assert_eq!(forecast(&history[..1], Some(&weather)), None);
    assert_eq!(forecast(&history[2..], Some(&weather)), None);
}

#[test]
fn same_clock_evidence_keeps_priority_over_neighbours() {
    let previous = target() - Duration::days(1);
    let mut history = vec![observation(previous, 0.1)];
    let mut weather = Weather(vec![(previous, 100.0), (target(), 800.0)]);
    for day in 1..=4 {
        for offset in [-30, 30] {
            let time = target() - Duration::days(day) + Duration::minutes(offset);
            history.push(observation(time, 0.9));
            weather.0.push((time, 100.0));
        }
    }
    assert_eq!(forecast(&history, Some(&weather)), Some(0.8));
}

#[test]
fn night_and_households_without_generation_stay_at_zero() {
    let previous = target() - Duration::days(1);
    let night = Weather(vec![(previous, 100.0), (target(), 0.0)]);
    assert_eq!(
        forecast(&[observation(previous, 0.5)], Some(&night)),
        Some(0.0)
    );
    assert_eq!(forecast(&[], Some(&night)), Some(0.0));
    let daylight = Weather(vec![(previous, 100.0), (target(), 800.0)]);
    assert_eq!(
        forecast(&[observation(previous, 0.0)], Some(&daylight)),
        Some(0.0)
    );
    assert_eq!(forecast(&[], Some(&daylight)), None);
}

#[test]
fn future_incomplete_and_expired_history_cannot_train_the_roof() {
    let history = [
        observation(target(), 100.0),
        observation(target() + Duration::days(1), 100.0),
        observation(target() - Duration::days(15), 100.0),
        observation(target() - Duration::days(1), 0.2),
    ];
    assert_eq!(forecast(&history, None), Some(0.2));
}

#[test]
fn historical_calibration_uses_only_weather_known_at_the_observation() {
    struct RevisedWeather;
    impl RadiationForecast for RevisedWeather {
        fn predicted(&self, time: DateTime<Utc>, known_at: DateTime<Utc>) -> Option<f64> {
            if time == target() {
                Some(800.0)
            } else if known_at == time {
                Some(100.0)
            } else {
                Some(1000.0)
            }
        }
    }
    let history = [observation(target() - Duration::days(1), 0.1)];
    assert_eq!(forecast(&history, Some(&RevisedWeather)), Some(0.8));
}

fn commissioning_history() -> (Vec<Observation>, Weather) {
    let mut history = Vec::new();
    let mut weather = Weather(vec![(target(), 800.0)]);
    for day in 1..=12 {
        for offset in [-30, 0, 30] {
            let time = target() - Duration::days(day) + Duration::minutes(offset);
            history.push(observation(time, if day <= 2 { 0.1 } else { 0.0 }));
            weather.0.push((time, 100.0));
        }
    }
    (history, weather)
}

#[test]
fn several_recent_production_periods_replace_pre_commissioning_zeros() {
    let (history, weather) = commissioning_history();
    // The full-window median remains zero despite two complete producing days.
    assert_eq!(forecast(&history, Some(&weather)), Some(0.0));
    let start = history_start(&history, target(), chrono_tz::UTC, Some(&weather)).unwrap();
    assert_eq!(start, target() - Duration::days(2) - Duration::minutes(30));
    let recent: Vec<_> = history
        .into_iter()
        .filter(|point| point.time >= start)
        .collect();
    assert_eq!(forecast(&recent, Some(&weather)), Some(0.8));
    assert_eq!(forecast(&recent, None), Some(0.1));
}

#[test]
fn single_spikes_and_a_single_producing_day_cannot_replace_zero_history() {
    let (mut history, weather) = commissioning_history();
    for point in &mut history {
        point.energy_kwh = 0.0;
    }
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, Some(&weather)),
        None
    );
    history[1].energy_kwh = 20.0;
    history[4].energy_kwh = 20.0;
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, Some(&weather)),
        None
    );
    for point in &mut history[..3] {
        point.energy_kwh = 0.1;
    }
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, Some(&weather)),
        None
    );
}

#[test]
fn cloudy_days_do_not_reset_a_previously_producing_roof() {
    let (mut history, weather) = commissioning_history();
    for point in &mut history {
        if point.time < target() - Duration::days(10) {
            point.energy_kwh = 0.1;
        }
    }
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, Some(&weather)),
        None
    );
}

#[test]
fn regime_change_requires_daylight_evidence_and_comparable_zero_day_coverage() {
    let (history, mut weather) = commissioning_history();
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, None),
        None
    );
    for (_, radiation) in &mut weather.0 {
        *radiation = 1.0;
    }
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, Some(&weather)),
        None
    );
    let (mut history, weather) = commissioning_history();
    for point in &mut history {
        if point.energy_kwh == 0.0 {
            point.time -= Duration::hours(6);
        }
    }
    assert_eq!(
        history_start(&history, target(), chrono_tz::UTC, Some(&weather)),
        None
    );
}

#[test]
fn regime_change_does_not_require_weather_archives_from_before_commissioning() {
    let (history, mut weather) = commissioning_history();
    weather
        .0
        .retain(|(time, _)| *time >= target() - Duration::days(2) - Duration::minutes(30));
    assert!(history_start(&history, target(), chrono_tz::UTC, Some(&weather)).is_some());
}

#[test]
fn commissioned_output_does_not_revert_to_old_zeros_as_the_transition_ages() {
    let onset = target();
    for elapsed_days in 2..=10 {
        let now = onset + Duration::days(elapsed_days);
        let mut history = Vec::new();
        let mut weather = Weather(vec![(now, 800.0)]);
        for day in -10..elapsed_days {
            for offset in [-30, 0, 30] {
                let time = onset + Duration::days(day) + Duration::minutes(offset);
                history.push(observation(time, if day < 0 { 0.0 } else { 0.1 }));
                weather.0.push((time, 100.0));
            }
        }
        let start = history_start(&history, now, chrono_tz::UTC, Some(&weather)).unwrap();
        let recent: Vec<_> = history
            .into_iter()
            .filter(|point| point.time >= start)
            .collect();
        assert_eq!(
            predict(&recent, now, now, chrono_tz::UTC, Some(&weather)),
            Some(0.8),
            "day {elapsed_days} after commissioning"
        );
    }
}
