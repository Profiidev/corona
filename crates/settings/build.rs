// i18n! embeds the locales at compile time but does not track them
fn main() {
  println!("cargo:rerun-if-changed=../../assets/locales");
}
