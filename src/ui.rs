macro_rules! slint_components {
  ($($name:ident($($component:ident),*)),*) => {
    $(
      pub mod $name {
        include!(concat!(env!("OUT_DIR"), "/", stringify!($name), ".rs"));

        $(
          impl super::SlintComponent for $component {}
        )*
      }
    )*
  };
}

pub trait SlintComponent {}

slint_components!(bar(Bar));
