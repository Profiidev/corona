use tracing::warn;

pub trait ErrorLogExt {
  fn log_err(self) -> Self;
}

impl<T, E: std::fmt::Debug> ErrorLogExt for Result<T, E> {
  fn log_err(self) -> Self {
    if let Err(e) = &self {
      warn!("{:?}", e);
    }
    self
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn returns_its_input() {
    assert_eq!(Ok::<_, &str>(1).log_err(), Ok(1));
    assert_eq!(Err::<i32, _>("boom").log_err(), Err("boom"));
  }
}
