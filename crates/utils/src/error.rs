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
