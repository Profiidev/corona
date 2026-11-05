use serde::Deserialize;

use crate::hyprland::command::hypr_data_cmd;

#[derive(Debug, Deserialize)]
pub struct Devices {
  keyboards: Vec<Keyboard>,
}

#[derive(Debug, Deserialize)]
struct Keyboard {
  /// Comma separated configured layouts, like `us,de`
  layout: String,
  active_keymap: String,
  main: bool,
}

fn main_keyboard(devices: Devices) -> Option<Keyboard> {
  let keyboards = devices.keyboards;
  let main = keyboards.iter().position(|k| k.main).unwrap_or(0);
  keyboards.into_iter().nth(main)
}

fn layout(devices: Devices) -> Option<String> {
  main_keyboard(devices).map(|keyboard| keyboard.active_keymap)
}

/// The main keyboard's configured layouts, like `["us", "de"]`
fn layouts(devices: Devices) -> Vec<String> {
  main_keyboard(devices)
    .map(|k| {
      k.layout
        .split(',')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
    })
    .unwrap_or_default()
}

hypr_data_cmd!(keyboard_layout, "devices", Devices, Option<String>, layout);
hypr_data_cmd!(keyboard_layouts, "devices", Devices, Vec<String>, layouts);

#[cfg(test)]
mod tests {
  #[test]
  fn main_keyboard_layout() {
    let devices = |json: &str| serde_json::from_str(json).unwrap();
    let both = r#"{ "keyboards": [
      { "layout": "us", "active_keymap": "English (US)", "main": false },
      { "layout": "de", "active_keymap": "German", "main": true }
    ] }"#;
    assert_eq!(super::layout(devices(both)).as_deref(), Some("German"));
    let no_main =
      r#"{ "keyboards": [{ "layout": "de", "active_keymap": "German", "main": false }] }"#;
    assert_eq!(super::layout(devices(no_main)).as_deref(), Some("German"));
    assert_eq!(super::layout(devices(r#"{ "keyboards": [] }"#)), None);
    // the first main keyboard wins
    let two_main = r#"{ "keyboards": [
      { "layout": "fr", "active_keymap": "French", "main": true },
      { "layout": "de", "active_keymap": "German", "main": true }
    ] }"#;
    assert_eq!(super::layout(devices(two_main)).as_deref(), Some("French"));
  }

  #[test]
  fn layouts() {
    let devices = serde_json::from_str(
      r#"{ "keyboards": [
        { "layout": "us", "active_keymap": "English (US)", "main": false },
        { "layout": "us, de", "active_keymap": "English (US)", "main": true }
      ] }"#,
    )
    .unwrap();
    assert_eq!(super::layouts(devices), ["us", "de"]);
  }

  #[test]
  fn keyboard_layout_over_ipc() {
    let hypr = crate::hyprland::fake::FakeHyprland::start();
    assert_eq!(
      hypr.ipc().keyboard_layout().unwrap().as_deref(),
      Some("German")
    );
    hypr.answer("j/devices", r#"{"mice": []}"#);
    assert!(hypr.ipc().keyboard_layout().is_err());
  }
}
