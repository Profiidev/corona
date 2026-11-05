use gpui_kit::{App, DisplayId};
use uuid::Uuid;

pub fn display_uuid(monitor: &str) -> Uuid {
  Uuid::new_v5(&Uuid::NAMESPACE_DNS, monitor.as_bytes())
}

pub fn display_id_for(monitor: &str, cx: &App) -> Option<DisplayId> {
  let uuid = display_uuid(monitor);
  cx.displays()
    .iter()
    .find(|d| d.uuid().ok() == Some(uuid))
    .map(|d| d.id())
}

/// Device-controlled text must not render as markdown: backslash-escapes the characters
/// markdown would interpret and turns line breaks into spaces
pub fn markdown_escape(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  for c in text.chars() {
    match c {
      '\n' | '\r' => out.push(' '),
      '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '<' | '>' | '!' | '|' | '#'
      | '~' => {
        out.push('\\');
        out.push(c)
      }
      c => out.push(c),
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn markdown_is_escaped() {
    assert_eq!(markdown_escape("~~a~~\n{b}"), "\\~\\~a\\~\\~ \\{b\\}");
  }

  #[test]
  fn uuids_are_stable_per_name() {
    assert_eq!(display_uuid("DP-1"), display_uuid("DP-1"));
    assert_ne!(display_uuid("DP-1"), display_uuid("DP-2"));
    assert_eq!(display_uuid("DP-1").get_version_num(), 5);
    // pinned: displays are matched across processes by this value
    assert_eq!(
      display_uuid("").to_string(),
      Uuid::new_v5(&Uuid::NAMESPACE_DNS, b"").to_string()
    );
    assert_ne!(display_uuid("ü"), display_uuid("u"));
  }

  #[gpui_kit::test]
  fn unknown_monitor_has_no_display(cx: &mut gpui_kit::TestAppContext) {
    // the test display has a random uuid, so no name maps to it
    cx.update(|cx| {
      assert!(!cx.displays().is_empty());
      assert_eq!(display_id_for("DP-1", cx), None);
      assert_eq!(display_id_for("", cx), None);
    });
  }
}
