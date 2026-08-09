macro_rules! slint_components {
  ($($name:ident($($component:ident),*)),*) => {
    $(
      pub mod $name {
        include!(concat!(env!("OUT_DIR"), "/", stringify!($name), ".rs"));

        $(
          impl crate::widgets::init::SlintComponent for $component {
            fn new() -> Result<Self, slint::PlatformError> {
              Self::new()
            }
          }
        )*
      }
    )*
  };
}

slint_components!(bar(Bar));
