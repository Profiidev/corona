use std::rc::Rc;

use corona_power::{PowerExt, SessionAction, SessionCapabilities, entry_title};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, Context, FocusHandle, Hsla, InteractiveElement, IntoElement, KeyDownEvent, ParentElement,
  Render, SharedString, StatefulInteractiveElement, Styled, Task, Window,
  assets::IconName,
  component::{ActiveTheme, Icon, scroll::ScrollableElement},
  div,
  prelude::FluentBuilder,
  px,
};

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
      Item::Lock => "Lock".into(),
      Item::Action(action) => match action {
        SessionAction::Logout => "Logout",
        SessionAction::Suspend => "Suspend",
        SessionAction::Hibernate => "Hibernate",
        SessionAction::SuspendThenHibernate => "Suspend then hibernate",
        SessionAction::Reboot => "Reboot",
        SessionAction::PowerOff => "Shutdown",
        SessionAction::RebootToFirmware => "Reboot to UEFI",
      }
      .into(),
      Item::RebootTo => "Reboot to…".into(),
      Item::BootEntry(entry) => entry_title(entry),
      Item::Back => "Back".into(),
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
      Item::Lock => false,
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
  pub const HEIGHT: f32 = ROW * ROWS as f32;

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

impl SessionMenu {
  fn entries(&self) -> Vec<Item> {
    match (self.boot_menu, &self.capabilities) {
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
    }
  }

  fn enabled(&self, entry: &Item) -> bool {
    match &self.capabilities {
      Some(capabilities) => entry.enabled(capabilities),
      None => entry == &Item::Action(SessionAction::Logout),
    }
  }

  fn activate(&mut self, entry: Item, window: &mut Window, cx: &mut Context<Self>) {
    if !self.enabled(&entry) {
      return;
    }
    if matches!(entry, Item::RebootTo | Item::Back) {
      self.boot_menu = entry == Item::RebootTo;
      return cx.notify();
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
    let enabled = self.enabled(&entry);
    let danger = entry == Item::Action(SessionAction::PowerOff);
    let color: Hsla = match danger {
      true => theme.colors.danger,
      false => theme.foreground,
    };
    let label = entry.label();
    let hover = theme.tokens.button_hover;

    div()
      .id(SharedString::from(format!("session-{index}")))
      .h(px(ROW))
      .flex_none()
      .flex()
      .items_center()
      .gap_3()
      .px_3()
      .rounded_lg()
      .text_color(color)
      .when(!enabled, |d| d.opacity(0.4))
      .when(enabled, |d| d.cursor_pointer().hover(move |d| d.bg(hover)))
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
      .size_full()
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
