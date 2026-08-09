use std::marker::PhantomData;

use slint::PlatformError;

pub struct InitFunction<F: FnOnce(&mut C), C: SlintComponent> {
  f: Option<F>,
  phantom: PhantomData<C>,
}

#[derive(Debug, thiserror::Error)]
pub enum WidgetInitError {
  #[error("Slint component already initialized")]
  AlreadyInitialized,
  #[error("Slint component initialization failed: {0}")]
  PlatformError(#[from] PlatformError),
}

pub trait IntoSlintInit<C> {
  type Init: SlintComponentInit + 'static;

  fn into_init(self) -> Self::Init;
}

impl<F: FnOnce(&mut C) + 'static, C: SlintComponent> IntoSlintInit<C> for F {
  type Init = InitFunction<Self, C>;

  fn into_init(self) -> Self::Init {
    InitFunction {
      f: Some(self),
      phantom: PhantomData,
    }
  }
}

pub trait SlintComponent: 'static {
  fn new() -> Result<Self, PlatformError>
  where
    Self: Sized;
}

pub trait SlintComponentInit {
  fn init(&mut self) -> Result<Box<dyn SlintComponent>, WidgetInitError>;
}

impl<F: FnOnce(&mut C), C: SlintComponent> SlintComponentInit for InitFunction<F, C> {
  fn init(&mut self) -> Result<Box<dyn SlintComponent>, WidgetInitError> {
    let f = self.f.take().ok_or(WidgetInitError::AlreadyInitialized)?;
    let mut component = C::new()?;
    f(&mut component);
    Ok(Box::new(component))
  }
}
