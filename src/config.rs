use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WeatherConfig {
    pub latitude: f64,
    pub longitude: f64,
    pub api_key: String,
}
impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            latitude: 53.96,
            longitude: -1.08,
            api_key: String::new(),
        }
    }
}
impl WeatherConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !self.latitude.is_finite()
            || !(-90.0..=90.0).contains(&self.latitude)
            || !self.longitude.is_finite()
            || !(-180.0..=180.0).contains(&self.longitude)
        {
            return Err("weather coordinates must be valid latitude and longitude".into());
        }
        Ok(())
    }
}
