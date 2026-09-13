//! Live radiation forecasts, retained with their actual receipt times.
use super::{model::HISTORY_DAYS as SOLAR_HISTORY_DAYS, RadiationForecast};
use crate::WeatherConfig;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WeatherRun {
    pub received_at: DateTime<Utc>,
    pub hours: Vec<(i64, f64)>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Weather {
    runs: Vec<WeatherRun>,
}
impl Weather {
    pub fn add(&mut self, run: WeatherRun) {
        self.runs.retain(|r| r.received_at != run.received_at);
        self.runs.push(run);
        self.runs.sort_by_key(|r| r.received_at);
        let newest = self
            .runs
            .last()
            .expect("just added a weather run")
            .received_at;
        // Solar calibration uses fourteen days of observations, including the
        // forecast received before the oldest observation began.
        self.runs
            .retain(|r| r.received_at >= newest - Duration::days(SOLAR_HISTORY_DAYS + 1));
    }
}
impl RadiationForecast for Weather {
    fn predicted(&self, time: DateTime<Utc>, known_at: DateTime<Utc>) -> Option<f64> {
        let end = time.timestamp().div_euclid(3600) * 3600 + 3600;
        self.runs
            .iter()
            .rev()
            .filter(|run| {
                run.received_at <= known_at && known_at - run.received_at <= Duration::hours(6)
            })
            .find_map(|run| {
                run.hours
                    .binary_search_by_key(&end, |row| row.0)
                    .ok()
                    .map(|i| run.hours[i].1)
            })
    }
}

#[derive(Deserialize)]
struct Response {
    hourly: Hourly,
}
#[derive(Deserialize)]
struct Hourly {
    time: Vec<i64>,
    shortwave_radiation: Vec<Option<f64>>,
}
impl WeatherRun {
    pub fn fetch(cfg: &WeatherConfig) -> Result<Self, String> {
        let key = if cfg.api_key.is_empty() {
            std::env::var("KWC_OPEN_METEO_API_KEY").unwrap_or_default()
        } else {
            cfg.api_key.clone()
        };
        let host = if key.is_empty() {
            "api.open-meteo.com"
        } else {
            "customer-api.open-meteo.com"
        };
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(5))
            .build();
        let mut request = agent
            .get(&format!("https://{host}/v1/forecast"))
            .query("latitude", &cfg.latitude.to_string())
            .query("longitude", &cfg.longitude.to_string())
            .query("hourly", "shortwave_radiation")
            .query("timeformat", "unixtime")
            .query("timezone", "GMT")
            .query("forecast_days", "3");
        if !key.is_empty() {
            request = request.query("apikey", &key);
        }
        // Never include a credential-bearing request URL in logs.
        let response: Response = request
            .call()
            .map_err(|_| "weather request failed")?
            .into_json()
            .map_err(|_| "invalid weather response")?;
        Self::from_hourly(response.hourly, Utc::now())
    }
    fn from_hourly(hourly: Hourly, received_at: DateTime<Utc>) -> Result<Self, String> {
        if hourly.time.len() != hourly.shortwave_radiation.len()
            || hourly.time.windows(2).any(|w| w[1] - w[0] != 3600)
            || hourly.time.iter().any(|t| t % 3600 != 0)
            || hourly
                .shortwave_radiation
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0.0..=2000.0).contains(v))
        {
            return Err("invalid weather hours".into());
        }
        let hours = hourly
            .time
            .into_iter()
            .zip(hourly.shortwave_radiation)
            .filter_map(|(time, value)| value.map(|v| (time, v)))
            .collect::<Vec<_>>();
        if !hours.iter().any(|(end, _)| *end > received_at.timestamp()) {
            return Err("weather response has no future radiation".into());
        }
        Ok(Self { received_at, hours })
    }
}

#[cfg(test)]
#[path = "../tests/unit/weather.rs"]
mod tests;
