use serde::Deserialize;

use crate::hypr_data_cmd;

#[derive(Debug, Deserialize)]
pub struct Devices {
  keyboards: Vec<Keyboard>,
}

#[derive(Debug, Deserialize)]
struct Keyboard {
  active_keymap: String,
  main: bool,
}

fn layout(devices: Devices) -> Option<String> {
  let keyboards = devices.keyboards;
  let main = keyboards.iter().position(|k| k.main).unwrap_or(0);
  keyboards
    .into_iter()
    .nth(main)
    .map(|keyboard| keyboard.active_keymap)
}

hypr_data_cmd!(keyboard_layout, "devices", Devices, Option<String>, layout);

#[cfg(test)]
mod tests {
  #[test]
  fn main_keyboard_layout() {
    let devices = |json: &str| serde_json::from_str(json).unwrap();
    let both = r#"{ "keyboards": [
      { "active_keymap": "English (US)", "main": false },
      { "active_keymap": "German", "main": true }
    ] }"#;
    assert_eq!(super::layout(devices(both)).as_deref(), Some("German"));
    let no_main = r#"{ "keyboards": [{ "active_keymap": "German", "main": false }] }"#;
    assert_eq!(super::layout(devices(no_main)).as_deref(), Some("German"));
    assert_eq!(super::layout(devices(r#"{ "keyboards": [] }"#)), None);
  }
}
