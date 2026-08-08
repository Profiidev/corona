//! Each `slint_build::compile()` call in build.rs writes its own generated file to OUT_DIR
//! (named after the source file's stem). `slint::include_modules!()` only ever sees the last one
//! compiled, so with more than one .slint file we `include!` each generated file into its own
//! module instead.

pub mod bar {
  include!(concat!(env!("OUT_DIR"), "/bar.rs"));
}
pub mod notification {
  include!(concat!(env!("OUT_DIR"), "/notification.rs"));
}
pub mod osd {
  include!(concat!(env!("OUT_DIR"), "/osd.rs"));
}
pub mod calendar {
  include!(concat!(env!("OUT_DIR"), "/calendar.rs"));
}
