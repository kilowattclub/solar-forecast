use super::*;
use chrono::TimeZone;

#[test]
fn delayed_weather_runs_preserve_newer_predictions_and_two_weeks_of_calibration() {
    let now = Utc.with_ymd_and_hms(2026, 9, 5, 10, 0, 0).unwrap();
    let end = (now + Duration::hours(1)).timestamp();
    let mut weather = Weather::default();
    for (age, value) in [(0, 500.0), (14, 100.0), (16, 900.0), (1, 300.0)] {
        weather.add(WeatherRun {
            received_at: now - Duration::days(age),
            hours: vec![(end, value)],
        });
    }
    assert_eq!(weather.predicted(now, now), Some(500.0));
    assert_eq!(
        weather.predicted(now, now - Duration::days(14)),
        Some(100.0)
    );
    assert_eq!(weather.predicted(now, now - Duration::days(16)), None);
    assert_eq!(weather.runs.len(), 3);
}

#[test]
fn newer_forecasts_cannot_rewrite_historical_calibration_and_stale_runs_expire() {
    let now = Utc.with_ymd_and_hms(2026, 9, 5, 10, 0, 0).unwrap();
    let end = (now + Duration::hours(1)).timestamp();
    let mut weather = Weather::default();
    weather.add(WeatherRun {
        received_at: now - Duration::hours(1),
        hours: vec![(end, 100.0)],
    });
    weather.add(WeatherRun {
        received_at: now + Duration::hours(1),
        hours: vec![(end, 900.0)],
    });
    assert_eq!(weather.predicted(now, now), Some(100.0));
    assert_eq!(
        weather.predicted(now, now + Duration::hours(1)),
        Some(900.0)
    );
    assert_eq!(weather.predicted(now, now + Duration::hours(8)), None);
    assert_eq!(weather.predicted(now, now - Duration::hours(2)), None);
}
#[test]
fn radiation_uses_hour_end_timestamps_and_nullable_hours_are_missing() {
    let now = Utc.with_ymd_and_hms(2026, 9, 5, 10, 0, 0).unwrap();
    let mut weather = Weather::default();
    weather.add(
        WeatherRun::from_hourly(
            Hourly {
                time: vec![now.timestamp() + 3600, now.timestamp() + 7200],
                shortwave_radiation: vec![Some(200.0), None],
            },
            now,
        )
        .unwrap(),
    );
    assert_eq!(weather.predicted(now, now), Some(200.0));
    assert_eq!(
        weather.predicted(now + Duration::minutes(30), now),
        Some(200.0)
    );
    assert_eq!(weather.predicted(now + Duration::hours(1), now), None);
}
