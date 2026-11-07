use serde::{Deserialize, Serialize};

/// A place on Earth, as configured, found by GeoClue or geocoded from a city
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Location {
  /// Empty for the device's own position, which the shell names
  pub name: String,
  pub latitude: f64,
  pub longitude: f64,
  pub query: Option<String>,
}
