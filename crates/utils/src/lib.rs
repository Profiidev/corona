pub mod bus;
pub mod dbus;
pub mod display;
pub mod entity;
pub mod error;
pub mod level;
pub mod persist;
pub mod stream;
#[cfg(feature = "test-support")]
pub mod test_bus;
pub mod ticker;

pub use bus::Buses;
pub use level::parse_level;

#[doc(hidden)]
pub use gpui_kit as __gpui_kit;

/// `pub trait $ext { fn $name(&self) -> &$ty }` for `App`, reading the global `$ty`
#[macro_export]
macro_rules! global_ext {
  ($(#[$meta:meta])* $ext:ident, $name:ident, $ty:ty) => {
    $(#[$meta])*
    pub trait $ext {
      fn $name(&self) -> &$ty;
    }

    impl $ext for $crate::__gpui_kit::App {
      fn $name(&self) -> &$ty {
        self.global::<$ty>()
      }
    }
  };
}
