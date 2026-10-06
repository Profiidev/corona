use anyhow::{Context, Result, bail};
use corona_ipc::IpcServer;

pub mod brightness;

pub mod media;
pub mod notification;
pub mod power_profile;
pub mod radio;
pub mod session;
pub mod theme;
pub mod volume;

pub fn register_commands(server: &mut IpcServer) {
  brightness::register_commands(server);
  media::register_commands(server);
  notification::register_commands(server);
  power_profile::register_commands(server);
  radio::register_commands(server);
  session::register_commands(server);
  theme::register_commands(server);
  volume::register_commands(server);
}

/// `65` and `65%` as 0.65, values up to 1 without `%` as they are
pub fn parse_level(s: &str) -> Result<f32> {
  let (number, percent) = match s.trim().strip_suffix('%') {
    Some(number) => (number, true),
    None => (s.trim(), false),
  };
  let value: f32 = number
    .trim()
    .parse()
    .context("expected a number like 65, 65% or 0.65")?;
  if !value.is_finite() || value < 0. {
    bail!("expected a positive number");
  }
  Ok(if percent || value > 1. {
    value / 100.
  } else {
    value
  })
}

#[cfg(test)]
mod tests {
  use super::parse_level;

  #[test]
  fn levels() {
    assert_eq!(parse_level("65").unwrap(), 0.65);
    assert_eq!(parse_level("65%").unwrap(), 0.65);
    assert_eq!(parse_level("0.65").unwrap(), 0.65);
    assert_eq!(parse_level("0.5%").unwrap(), 0.005);
    assert_eq!(parse_level("1").unwrap(), 1.);
    assert!(parse_level("-5").is_err());
    assert!(parse_level("loud").is_err());
  }
}
