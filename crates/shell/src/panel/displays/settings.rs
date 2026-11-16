//! Choosing a mode and a scale for one monitor, free of any UI so it can be tested

use corona_compositor::types::Mode;

pub(crate) const MIN_PERCENT: f32 = 50.;
pub(crate) const MAX_PERCENT: f32 = 300.;

/// The sizes the modes come in, largest first
pub(crate) fn resolutions(modes: &[Mode]) -> Vec<(u32, u32)> {
  let mut sizes: Vec<_> = modes.iter().map(|m| (m.width, m.height)).collect();
  sizes.sort_by(|a, b| b.cmp(a));
  sizes.dedup();
  sizes
}

/// The modes of one size, fastest first
pub(crate) fn rates(modes: &[Mode], size: (u32, u32)) -> Vec<Mode> {
  let mut at: Vec<Mode> = modes
    .iter()
    .copied()
    .filter(|m| (m.width, m.height) == size)
    .collect();
  at.sort_by_key(|m| std::cmp::Reverse(m.millihertz()));
  at
}

/// The mode of `size` to use after the size changed: the one at the rate the
/// monitor ran at when it has one, else the fastest
pub(crate) fn mode_for(modes: &[Mode], size: (u32, u32), current_rate: f32) -> Option<Mode> {
  let at = rates(modes, size);
  at.iter()
    .find(|m| (m.refresh_rate - current_rate).abs() < 0.5)
    .or(at.first())
    .copied()
}

/// A scale from a percentage the user typed: kept in range and put on a
/// multiple of 1/120, which is all fractional-scale clients can follow
pub(crate) fn scale_from_percent(percent: f32) -> f32 {
  let percent = if percent.is_finite() { percent } else { 100. };
  let scale = percent.clamp(MIN_PERCENT, MAX_PERCENT) / 100.;
  (scale * 120.).round() / 120.
}

pub(crate) fn percent(scale: f32) -> f32 {
  (scale * 100.).round()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn mode(width: u32, height: u32, refresh_rate: f32) -> Mode {
    Mode {
      width,
      height,
      refresh_rate,
    }
  }

  fn modes() -> Vec<Mode> {
    vec![
      mode(1280, 800, 60.),
      mode(1920, 1200, 48.),
      mode(1920, 1200, 60.),
      mode(1920, 1080, 60.),
      mode(1920, 1080, 144.),
    ]
  }

  #[test]
  fn resolutions_are_unique_and_largest_first() {
    assert_eq!(
      resolutions(&modes()),
      [(1920, 1200), (1920, 1080), (1280, 800)]
    );
    assert!(resolutions(&[]).is_empty());
  }

  #[test]
  fn rates_are_fastest_first() {
    let at: Vec<_> = rates(&modes(), (1920, 1080))
      .iter()
      .map(|m| m.refresh_rate)
      .collect();
    assert_eq!(at, [144., 60.]);
    assert!(rates(&modes(), (1, 1)).is_empty());
  }

  #[test]
  fn a_new_size_keeps_the_rate_when_it_can() {
    // 60 Hz exists at both sizes
    assert_eq!(
      mode_for(&modes(), (1920, 1080), 60.).unwrap().refresh_rate,
      60.
    );
    // the rate the 59.95 Hz monitor ran at still counts as 60
    assert_eq!(
      mode_for(&modes(), (1920, 1080), 59.95)
        .unwrap()
        .refresh_rate,
      60.
    );
    // 144 Hz does not exist at the new size: the fastest there
    assert_eq!(
      mode_for(&modes(), (1920, 1200), 144.).unwrap().refresh_rate,
      60.
    );
    assert!(mode_for(&modes(), (1, 1), 60.).is_none());
  }

  #[test]
  fn scale_is_clamped_and_snapped_to_120ths() {
    assert_eq!(scale_from_percent(100.), 1.);
    assert_eq!(scale_from_percent(125.), 1.25);
    assert_eq!(scale_from_percent(10.), 0.5);
    assert_eq!(scale_from_percent(999.), 3.);
    assert_eq!(scale_from_percent(f32::NAN), 1.);
    // 133% is not a multiple of 1/120; the nearest one is 160/120
    let s = scale_from_percent(133.);
    assert!(((s * 120.).round() - s * 120.).abs() < 1e-4);
    assert!((s - 1.33).abs() < 0.005);
    assert_eq!(percent(1.2), 120.);
  }
}
