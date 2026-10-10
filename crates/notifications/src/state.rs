use std::{path::PathBuf, time::SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
  Low,
  Normal,
  Critical,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Action {
  pub key: String,
  pub label: String,
}

/// What to draw for a notification: a picture file or a theme icon name
#[derive(Clone, Debug, PartialEq)]
pub enum NotificationImage {
  Path(PathBuf),
  Name(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
  pub id: u32,
  pub app_name: String,
  /// a theme icon name or a `file://` path, empty when the app sent none
  pub app_icon: String,
  /// from `image-data`, `image-path` or `app_icon`, the first one set
  pub image: Option<NotificationImage>,
  pub summary: String,
  /// may hold markup: `b`, `i`, `u`, `a` and `img` tags, see [`strip_markup`]
  pub body: String,
  pub actions: Vec<Action>,
  pub urgency: Urgency,
  pub desktop_entry: Option<String>,
  /// the `x-kde-reply-placeholder-text` hint, for an `inline-reply` action
  pub reply_placeholder: Option<String>,
  pub resident: bool,
  pub time: SystemTime,
  pub read: bool,
}

/// The body markup as plain text: tags dropped, entities decoded
pub fn strip_markup(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(start) = rest.find('<') {
    out.push_str(&rest[..start]);
    match rest[start..].find('>') {
      Some(end) => rest = &rest[start + end + 1..],
      // a lone `<` is text
      None => {
        out.push_str(&rest[start..]);
        rest = "";
      }
    }
  }
  out.push_str(rest);
  unescape(&out)
}

fn unescape(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(start) = rest.find('&') {
    out.push_str(&rest[..start]);
    rest = &rest[start..];
    let entity = rest.find(';').map(|end| (&rest[1..end], end));
    let decoded = entity.and_then(|(name, end)| {
      let c = match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => {
          let code = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
            Some(hex) => u32::from_str_radix(hex, 16).ok(),
            None => name.strip_prefix('#')?.parse().ok(),
          };
          char::from_u32(code?)?
        }
      };
      Some((c, end))
    });
    match decoded {
      Some((c, end)) => {
        out.push(c);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out
}

pub(crate) fn actions(flat: Vec<String>) -> Vec<Action> {
  flat
    .as_chunks::<2>()
    .0
    .iter()
    .map(|[key, label]| Action {
      key: key.clone(),
      label: label.clone(),
    })
    .collect()
}

pub(crate) fn mark_read(list: &mut [Notification]) -> bool {
  let changed = list.iter().any(|n| !n.read);
  list.iter_mut().for_each(|n| n.read = true);
  changed
}

pub(crate) fn insert(list: &mut Vec<Notification>, notification: Notification) {
  list.retain(|n| n.id != notification.id);
  list.insert(0, notification);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn notification(id: u32, summary: &str) -> Notification {
    Notification {
      id,
      app_name: "test".into(),
      app_icon: String::new(),
      image: None,
      summary: summary.into(),
      body: String::new(),
      actions: Vec::new(),
      urgency: Urgency::Normal,
      desktop_entry: None,
      reply_placeholder: None,
      resident: false,
      time: SystemTime::UNIX_EPOCH,
      read: false,
    }
  }

  #[test]
  fn helpers() {
    let parsed = actions(vec!["default".into(), "Open".into(), "dangling".into()]);
    assert_eq!(
      parsed,
      [Action {
        key: "default".into(),
        label: "Open".into()
      }]
    );

    let mut list = Vec::new();
    insert(&mut list, notification(1, "first"));
    insert(&mut list, notification(2, "second"));
    insert(&mut list, notification(1, "first, updated"));
    let summaries: Vec<_> = list.iter().map(|n| n.summary.as_str()).collect();
    assert_eq!(summaries, ["first, updated", "second"]);

    assert!(mark_read(&mut list));
    assert!(!mark_read(&mut list));
    // a replacement is new content, so it is unread again
    insert(&mut list, notification(2, "second, updated"));
    assert!(list.iter().any(|n| !n.read));
  }

  #[test]
  fn strip_markup() {
    let strip = super::strip_markup;
    assert_eq!(
      strip("<b>bold</b> and <a href=\"https://x.y/?a=1&amp;b=2\">link</a>"),
      "bold and link"
    );
    assert_eq!(strip("pic <img src=\"/a.png\" alt=\"x\"/> end"), "pic  end");
    assert_eq!(
      strip("1 &lt; 2 &amp;&amp; 3 &gt; 2 &quot;&apos;"),
      "1 < 2 && 3 > 2 \"'"
    );
    assert_eq!(strip("&#65;&#x42;&#X43;"), "ABC");
    // unknown entities, a lone `&` and a lone `<` stay
    assert_eq!(
      strip("a & b &bogus; &#xzz; c < d"),
      "a & b &bogus; &#xzz; c < d"
    );
    // decoding happens once, after the tags are gone
    assert_eq!(strip("&lt;b&gt;"), "<b>");
    assert_eq!(strip("line\nbreak"), "line\nbreak");
  }

  #[test]
  fn helper_edges() {
    assert!(actions(vec![]).is_empty());
    assert!(actions(vec!["only".into()]).is_empty());
    assert_eq!(
      actions(vec!["a".into(), "".into(), "b".into(), "B".into()]).len(),
      2
    );
    assert!(!mark_read(&mut []));
    // a new id goes first
    let mut list = vec![notification(1, "a")];
    insert(&mut list, notification(9, "b"));
    assert_eq!(list.iter().map(|n| n.id).collect::<Vec<_>>(), [9, 1]);
    assert!(Urgency::Critical > Urgency::Normal && Urgency::Normal > Urgency::Low);
  }

  #[test]
  fn actions_duplicate_keys_and_empty_entries() {
    let parsed = actions(vec![
      "".into(),
      "".into(),
      "dup".into(),
      "First".into(),
      "dup".into(),
      "Second".into(),
    ]);
    assert_eq!(parsed.len(), 3);
    assert_eq!(
      parsed,
      [
        Action {
          key: "".into(),
          label: "".into(),
        },
        Action {
          key: "dup".into(),
          label: "First".into(),
        },
        Action {
          key: "dup".into(),
          label: "Second".into(),
        },
      ]
    );
  }

  #[test]
  fn unbounded_insert_and_collision_replacement() {
    let mut list = Vec::new();
    for i in 1..=100 {
      insert(&mut list, notification(i, &format!("notif {i}")));
    }
    assert_eq!(list.len(), 100);
    // MRU order: last inserted is at index 0
    assert_eq!(list[0].id, 100);
    assert_eq!(list[99].id, 1);

    // If a wraparound collision occurs and ID 1 is re-inserted:
    insert(&mut list, notification(1, "wrapped notification 1"));
    assert_eq!(list.len(), 100);
    assert_eq!(list[0].id, 1);
    assert_eq!(list[0].summary, "wrapped notification 1");
    // ID 1 should only appear once in the list
    assert_eq!(list.iter().filter(|n| n.id == 1).count(), 1);
  }
}
