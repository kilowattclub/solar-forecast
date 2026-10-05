# solar-forecast

Solar energy forecasts calibrated against measured generation and causal
radiation forecasts. No Brain configuration, inverter, private repositories or
other Kilowatt Club packages are required.

```rust,no_run
use chrono::{DateTime, Duration, Utc};
use solar_forecast::{predict, History, Observation};

fn main() -> Result<(), String> {
    let now = Utc::now();
    let mut history = History::open("solar-forecast-history.json", now)?;

    // Record a measured, fully completed, UTC-aligned half-hour, in kWh.
    let aligned = now.timestamp().div_euclid(1800) * 1800;
    let start = DateTime::from_timestamp(aligned - 1800, 0).unwrap();
    history.record(Observation { time: start, energy_kwh: 0.4 }, now)?;

    let from = start + Duration::minutes(30);
    let forecast = predict(
        history.observations().iter().copied(),
        from..from + Duration::days(1),
        now,
        chrono_tz::Europe::London,
        None, // weather
        None, // current reading
    );
    assert_eq!(forecast.len(), 48);
    Ok(())
}
```

`predict` returns full half-hour energy values in kWh, in order from the requested
horizon start. Use aligned UTC half-hours. `Reading { at, power_kw }` can override
the current slot with a recent measured power reading; stale readings expire.

Solar calibration uses weather known at the time of each observation and robust
same-clock history when weather is unavailable. `WeatherRun::fetch` supports
Open-Meteo, and `Weather::add` retains forecast receipt times. Latitude/longitude
and optional API key belong to `WeatherConfig`.

`History` records **completed measured energy**, not synthetic data or a forecast.
It atomically persists at most 28 days of valid complete slots, replaces duplicate
timestamps, and rejects partial/future/non-finite/negative observations. Missing
files begin empty; corrupt files report an error. Use one writer per file. Callers
can also supply their own history directly without using filesystem storage.

## Development and release

Run `cargo test --locked`, `cargo clippy --locked --all-targets -- --deny warnings`,
and `cargo package --locked`. CI tests the library and its package independently.
Prepared for crates.io; not yet published. Consumers currently pin a Git revision.
When ready, review the package, publish with `cargo publish --locked`, and tag the
released version. MIT licensed.
