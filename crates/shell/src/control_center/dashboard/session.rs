use std::rc::Rc;

use corona_power::{EntryTitle, PowerExt, SessionAction, SessionCapabilities, entry_title};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, Context, FocusHandle, Hsla, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
  Render, StatefulInteractiveElement, Styled, Task, Window,
  assets::IconName,
  component::{ActiveTheme, Icon, scroll::ScrollableElement},
  div,
  prelude::FluentBuilder,
  px,
};

use crate::lock::LockState;
use rust_i18n::t;

const ROW: f32 = 40.;
const ROWS: usize = 9;

#[derive(Clone, Debug, PartialEq)]
enum Item {
  Lock,
  Action(SessionAction),
  RebootTo,
  BootEntry(String),
  Back,
}

impl Item {
  fn label(&self) -> String {
    match self {
      Item::Lock => t!("app.session.lock").into(),
      Item::Action(action) => match action {
        SessionAction::Logout => t!("app.session.logout"),
        SessionAction::Suspend => t!("app.session.suspend"),
        SessionAction::Hibernate => t!("app.session.hibernate"),
        SessionAction::SuspendThenHibernate => t!("app.session.suspend_then_hibernate"),
        SessionAction::Reboot => t!("app.session.reboot"),
        SessionAction::PowerOff => t!("app.session.shutdown"),
        SessionAction::RebootToFirmware => t!("app.session.reboot_to_firmware"),
      }
      .into(),
      Item::RebootTo => t!("app.session.reboot_to").into(),
      Item::BootEntry(entry) => match entry_title(entry) {
        EntryTitle::NixosGeneration(generation) => {
          t!("app.session.nixos_generation", generation = generation).into()
        }
        EntryTitle::Other(title) => title,
      },
      Item::Back => t!("app.common.back").into(),
    }
  }

  fn icon(&self) -> IconName {
    match self {
      Item::Lock => IconName::Lock,
      Item::Action(SessionAction::Logout) => IconName::LogOut,
      Item::Action(SessionAction::Suspend) => IconName::Pause,
      Item::Action(SessionAction::Hibernate) => IconName::Snowflake,
      Item::Action(SessionAction::SuspendThenHibernate) => IconName::Moon,
      Item::Action(SessionAction::Reboot) => IconName::RotateCw,
      Item::Action(SessionAction::PowerOff) => IconName::Power,
      Item::Action(SessionAction::RebootToFirmware) => IconName::Cpu,
      Item::RebootTo => IconName::ChevronRight,
      Item::BootEntry(_) => IconName::RotateCw,
      Item::Back => IconName::ChevronLeft,
    }
  }

  fn enabled(&self, capabilities: &SessionCapabilities) -> bool {
    match self {
      Item::Lock => true,
      Item::Action(action) => match action {
        SessionAction::Logout => true,
        SessionAction::Suspend => capabilities.suspend,
        SessionAction::Hibernate => capabilities.hibernate,
        SessionAction::SuspendThenHibernate => capabilities.suspend_then_hibernate,
        SessionAction::Reboot => capabilities.reboot,
        SessionAction::PowerOff => capabilities.power_off,
        SessionAction::RebootToFirmware => capabilities.reboot_to_firmware,
      },
      Item::RebootTo => !boot_entries(capabilities).is_empty(),
      Item::BootEntry(_) | Item::Back => true,
    }
  }
}

fn boot_entries(capabilities: &SessionCapabilities) -> Vec<Item> {
  capabilities
    .boot_entries
    .iter()
    .filter(|e| *e != "auto-reboot-to-firmware-setup")
    .map(|e| Item::BootEntry(e.clone()))
    .collect()
}

type OnDone = Rc<dyn Fn(&mut Window, &mut App)>;

pub struct SessionMenu {
  focus: FocusHandle,
  capabilities: Option<SessionCapabilities>,
  boot_menu: bool,
  on_done: OnDone,
  _load: Task<()>,
}
impl SessionMenu {
  pub const WIDTH: f32 = 300.;

  pub fn new(on_done: impl Fn(&mut Window, &mut App) + 'static, cx: &mut Context<Self>) -> Self {
    let task = cx.power().session_capabilities();
    let load = cx.spawn(async move |this, cx| {
      let capabilities = task.await.log_err().unwrap_or_default();
      let _ = this.update(cx, |this, cx| {
        this.capabilities = Some(capabilities);
        cx.notify();
      });
    });
    Self {
      focus: cx.focus_handle(),
      capabilities: None,
      boot_menu: false,
      on_done: Rc::new(on_done),
      _load: load,
    }
  }
}

/// The boot menu once capabilities loaded, the main menu otherwise. Only what
/// can run shows: before capabilities loaded that is locking and logging out
fn entries(boot_menu: bool, capabilities: Option<&SessionCapabilities>) -> Vec<Item> {
  let all = match (boot_menu, capabilities) {
    (true, Some(capabilities)) => std::iter::once(Item::Back)
      .chain(boot_entries(capabilities))
      .collect(),
    _ => vec![
      Item::Lock,
      Item::Action(SessionAction::Logout),
      Item::Action(SessionAction::Suspend),
      Item::Action(SessionAction::Hibernate),
      Item::Action(SessionAction::SuspendThenHibernate),
      Item::Action(SessionAction::Reboot),
      Item::RebootTo,
      Item::Action(SessionAction::RebootToFirmware),
      Item::Action(SessionAction::PowerOff),
    ],
  };
  all
    .into_iter()
    .filter(|entry| enabled(capabilities, entry))
    .collect()
}

/// Only locking and logging out work before capabilities loaded
fn enabled(capabilities: Option<&SessionCapabilities>, entry: &Item) -> bool {
  match capabilities {
    Some(capabilities) => entry.enabled(capabilities),
    None => matches!(entry, Item::Lock | Item::Action(SessionAction::Logout)),
  }
}

impl SessionMenu {
  fn entries(&self) -> Vec<Item> {
    entries(self.boot_menu, self.capabilities.as_ref())
  }

  fn activate(&mut self, entry: Item, window: &mut Window, cx: &mut Context<Self>) {
    if matches!(entry, Item::RebootTo | Item::Back) {
      self.boot_menu = entry == Item::RebootTo;
      return cx.notify();
    }
    if entry == Item::Lock {
      LockState::lock(cx).detach();
      return (self.on_done)(window, cx);
    }
    let power = cx.power();
    let task = match entry {
      Item::Action(action) => Some(power.session_action(action)),
      _ => None,
    };
    let reboot = match entry {
      Item::BootEntry(id) => Some(power.reboot_to(id)),
      _ => None,
    };
    cx.spawn(async move |_, _| {
      if let Some(task) = task {
        let _ = task.await.log_err();
      }
      if let Some(reboot) = reboot {
        let _ = reboot.await.log_err();
      }
    })
    .detach();
    (self.on_done)(window, cx);
  }

  fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    let key = e.keystroke.key.as_str();
    if key == "escape" {
      if self.boot_menu {
        self.boot_menu = false;
        return cx.notify();
      }
      return (self.on_done)(window, cx);
    }
    let number = key.parse::<usize>().ok().filter(|n| *n > 0);
    if let Some(entry) = number.and_then(|n| self.entries().into_iter().nth(n - 1)) {
      self.activate(entry, window, cx);
    }
  }

  fn row(&self, index: usize, entry: Item, cx: &mut Context<Self>) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let danger = entry == Item::Action(SessionAction::PowerOff);
    let color: Hsla = match danger {
      true => theme.colors.danger,
      false => theme.foreground,
    };
    let label = entry.label();
    let hover = theme.tokens.button_hover;

    div()
      .id(format!("session-{index}"))
      .h(px(ROW))
      .flex_none()
      .flex()
      .items_center()
      .gap_3()
      .px_3()
      .rounded_lg()
      .text_color(color)
      .cursor_pointer()
      .hover(move |d| d.bg(hover))
      .child(Icon::new(entry.icon()))
      .child(div().flex_1().min_w_0().truncate().child(label))
      .when(index < 9, |d| {
        d.child(
          div()
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(theme.tokens.button_hover)
            .text_xs()
            .text_color(theme.foreground)
            .child((index + 1).to_string()),
        )
      })
      .on_click(cx.listener(move |this, _, window, cx| this.activate(entry.clone(), window, cx)))
  }
}

impl Render for SessionMenu {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let rows: Vec<_> = self
      .entries()
      .into_iter()
      .enumerate()
      .map(|(i, entry)| self.row(i, entry, cx))
      .collect();

    div()
      .track_focus(&self.focus)
      .on_key_down(cx.listener(Self::on_key))
      .w_full()
      .h(px(ROW * rows.len().min(ROWS) as f32))
      .child(
        div()
          .size_full()
          .flex()
          .flex_col()
          .overflow_y_scrollbar()
          .children(rows),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn all() -> SessionCapabilities {
    SessionCapabilities {
      suspend: true,
      hibernate: true,
      suspend_then_hibernate: true,
      reboot: true,
      power_off: true,
      reboot_to_firmware: true,
      boot_entries: vec!["windows.conf".into()],
    }
  }

  #[test]
  fn main_menu_fits() {
    let main = entries(false, Some(&all()));
    assert_eq!(main.len(), ROWS);
    assert_eq!(main[0], Item::Lock);
    assert_eq!(main.last(), Some(&Item::Action(SessionAction::PowerOff)));
    // the boot menu needs capabilities
    assert_eq!(entries(true, None), entries(false, None));
  }

  #[test]
  fn unavailable_actions_hide() {
    let mut caps = all();
    caps.hibernate = false;
    caps.boot_entries.clear();
    let main = entries(false, Some(&caps));
    assert!(!main.contains(&Item::Action(SessionAction::Hibernate)));
    assert!(!main.contains(&Item::RebootTo));
    assert_eq!(main.len(), ROWS - 2);
    let none = entries(false, Some(&SessionCapabilities::default()));
    assert_eq!(none, [Item::Lock, Item::Action(SessionAction::Logout)]);
  }

  #[test]
  fn boot_menu() {
    let mut caps = all();
    caps.boot_entries = vec![
      "a.conf".into(),
      "auto-reboot-to-firmware-setup".into(),
      "b.conf".into(),
    ];
    assert_eq!(
      entries(true, Some(&caps)),
      [
        Item::Back,
        Item::BootEntry("a.conf".into()),
        Item::BootEntry("b.conf".into())
      ]
    );
  }

  #[test]
  fn nothing_loaded_shows_lock_and_logout() {
    assert_eq!(
      entries(false, None),
      [Item::Lock, Item::Action(SessionAction::Logout)]
    );
  }

  #[test]
  fn each_capability_gates_its_action() {
    type Flag = fn(&mut SessionCapabilities) -> &mut bool;
    let cases: [(SessionAction, Flag); 6] = [
      (SessionAction::Suspend, |c| &mut c.suspend),
      (SessionAction::Hibernate, |c| &mut c.hibernate),
      (SessionAction::SuspendThenHibernate, |c| {
        &mut c.suspend_then_hibernate
      }),
      (SessionAction::Reboot, |c| &mut c.reboot),
      (SessionAction::PowerOff, |c| &mut c.power_off),
      (SessionAction::RebootToFirmware, |c| {
        &mut c.reboot_to_firmware
      }),
    ];
    for (action, flag) in cases {
      let mut caps = all();
      assert!(Item::Action(action).enabled(&caps), "{action:?}");
      *flag(&mut caps) = false;
      assert!(!Item::Action(action).enabled(&caps), "{action:?}");
      // the others stay enabled
      let others = cases.iter().filter(|(a, _)| *a != action);
      assert!(others.clone().all(|(a, _)| Item::Action(*a).enabled(&caps)));
    }
    let none = SessionCapabilities::default();
    for item in [
      Item::Lock,
      Item::Action(SessionAction::Logout),
      Item::Back,
      Item::BootEntry("x".into()),
    ] {
      assert!(item.enabled(&none), "{item:?}");
    }
  }

  #[test]
  fn reboot_to_needs_real_entries() {
    let mut caps = all();
    assert!(Item::RebootTo.enabled(&caps));
    caps.boot_entries = vec!["auto-reboot-to-firmware-setup".into()];
    assert!(!Item::RebootTo.enabled(&caps));
    caps.boot_entries.clear();
    assert!(!Item::RebootTo.enabled(&caps));
  }

  #[test]
  fn labels() {
    assert_eq!(
      Item::BootEntry("nixos-generation-42.conf".into()).label(),
      "NixOS generation 42"
    );
    assert_eq!(Item::BootEntry("windows.conf".into()).label(), "Windows");
    let mut labels: Vec<_> = entries(false, Some(&all()))
      .iter()
      .chain([&Item::Back])
      .map(Item::label)
      .collect();
    let count = labels.len();
    labels.sort();
    labels.dedup();
    assert_eq!(labels.len(), count);
  }

  #[test]
  fn icons() {
    let mut icons: Vec<_> = entries(false, Some(&all()))
      .iter()
      .map(Item::icon)
      .collect();
    let count = icons.len();
    icons.sort_by_key(|i| format!("{i:?}"));
    icons.dedup();
    assert_eq!(icons.len(), count);
    assert_eq!(Item::Back.icon(), IconName::ChevronLeft);
    assert_eq!(Item::BootEntry("x".into()).icon(), IconName::RotateCw);
  }
}
