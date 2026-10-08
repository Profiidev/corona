use std::time::SystemTime;

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

#[derive(Clone, Debug, PartialEq)]
pub struct Notification {
  pub id: u32,
  pub app_name: String,
  /// a theme icon name or a `file://` path, empty when the app sent none
  pub app_icon: String,
  pub summary: String,
  pub body: String,
  pub actions: Vec<Action>,
  pub urgency: Urgency,
  pub desktop_entry: Option<String>,
  pub resident: bool,
  pub time: SystemTime,
  pub read: bool,
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
      summary: summary.into(),
      body: String::new(),
      actions: Vec::new(),
      urgency: Urgency::Normal,
      desktop_entry: None,
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
}
