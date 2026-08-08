macro_rules! slint_components {
  ($($name:ident),*) => {
    $(
      pub mod $name {
        include!(concat!(env!("OUT_DIR"), "/", stringify!($name), ".rs"));
      }
    )*
  };
}

slint_components!(bar);
