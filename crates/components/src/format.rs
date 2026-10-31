use crate::i18n::decimal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
  /// Powers of 1024 with one digit: `1.5 GiB`
  Binary,
  /// Powers of 1000 like drives are sold, a digit only below ten: `512 MB`, `1.2 TB`
  Decimal,
}

/// `n` bytes in the largest unit that keeps the number at least one
pub fn bytes(n: u64, base: Base) -> String {
  let (step, units) = match base {
    Base::Binary => (1024., ["B", "KiB", "MiB", "GiB", "TiB"]),
    Base::Decimal => (1000., ["B", "kB", "MB", "GB", "TB"]),
  };
  let mut value = n as f64;
  let mut unit = 0;
  while value >= step && unit < units.len() - 1 {
    value /= step;
    unit += 1;
  }
  let digits = match base {
    Base::Binary => 1,
    Base::Decimal => usize::from(value < 10. && unit > 0 && value.fract() >= 0.05),
  };
  format!("{} {}", decimal(value, digits), units[unit])
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn binary() {
    assert_eq!(bytes(0, Base::Binary), "0.0 B");
    assert_eq!(bytes(1536, Base::Binary), "1.5 KiB");
    assert_eq!(bytes(3 << 30, Base::Binary), "3.0 GiB");
  }

  #[test]
  fn decimal_reads_like_the_box() {
    assert_eq!(bytes(512, Base::Decimal), "512 B");
    assert_eq!(bytes(512_000_000, Base::Decimal), "512 MB");
    assert_eq!(bytes(1_000_204_886_016, Base::Decimal), "1 TB");
    assert_eq!(bytes(1_200_000_000_000, Base::Decimal), "1.2 TB");
    assert_eq!(bytes(32_000_000_000, Base::Decimal), "32 GB");
    assert_eq!(bytes(0, Base::Decimal), "0 B");
  }
}
