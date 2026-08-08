fn main() {
  for file in ["bar", "notification", "osd", "calendar"] {
    slint_build::compile(format!("ui/{file}.slint"))
      .unwrap_or_else(|e| panic!("failed to compile ui/{file}.slint: {e}"));
  }
}
