use std::rc::Rc;

use anyhow::Result;
use corona_power::{PowerExt, SessionAction, SessionCapabilities};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Anchor, App, AppContext, Context, Entity, FocusHandle, Focusable, Hsla, ImageSource,
  InteractiveElement, IntoElement, ParentElement, Render, SharedString, StatefulInteractiveElement,
  Styled, Subscription, Task, Window,
  assets::IconName,
  component::{
    ActiveTheme, Icon, Sizable,
    avatar::Avatar,
    button::Button,
    form::field,
    input::{
      InputEvent, InputGroup, InputGroupAddon, InputGroupAddonAlignment, InputGroupButton,
      InputState,
    },
    popover::Popover,
    separator::Separator,
    spinner::Spinner,
  },
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

const WIDTH: f32 = 320.;
const MENU_WIDTH: f32 = 248.;
const INSET: f32 = 32.;

/// What the screen is for. `Login` is the greeter: no log out, "Power" for the
/// menu and "log in" wording
#[derive(Clone, Copy, PartialEq)]
pub enum Purpose {
  Unlock,
  Login,
}

impl Purpose {
  fn submit(self) -> SharedString {
    match self {
      Purpose::Unlock => t!("app.lock.unlock").into(),
      Purpose::Login => t!("app.lock.login").into(),
    }
  }

  /// The session menu's button
  fn menu(self) -> SharedString {
    match self {
      Purpose::Unlock => t!("app.lock.session").into(),
      Purpose::Login => t!("app.lock.power").into(),
    }
  }

  fn idle(self) -> SharedString {
    match self {
      Purpose::Unlock => t!("app.lock.idle").into(),
      Purpose::Login => t!("app.lock.idle_login").into(),
    }
  }

  fn groups(self) -> Vec<Vec<SessionAction>> {
    use SessionAction as A;
    let sleep = vec![A::Suspend, A::Hibernate, A::SuspendThenHibernate];
    let power = vec![A::Reboot, A::PowerOff];
    match self {
      Purpose::Unlock => vec![vec![A::Logout], sleep, power],
      Purpose::Login => vec![sleep, power],
    }
  }
}

fn label(action: SessionAction) -> SharedString {
  match action {
    SessionAction::Logout => t!("app.session.logout"),
    SessionAction::Suspend => t!("app.session.suspend"),
    SessionAction::Hibernate => t!("app.session.hibernate"),
    SessionAction::SuspendThenHibernate => t!("app.session.suspend_then_hibernate"),
    SessionAction::Reboot => t!("app.session.reboot"),
    SessionAction::PowerOff => t!("app.session.shutdown"),
    SessionAction::RebootToFirmware => t!("app.session.reboot_to_firmware"),
  }
  .into()
}

fn busy(action: SessionAction) -> SharedString {
  match action {
    SessionAction::Logout => t!("app.lock.busy.logout"),
    SessionAction::Suspend => t!("app.lock.busy.suspend"),
    SessionAction::Hibernate => t!("app.lock.busy.hibernate"),
    SessionAction::SuspendThenHibernate => t!("app.lock.busy.suspend_then_hibernate"),
    SessionAction::Reboot | SessionAction::RebootToFirmware => t!("app.lock.busy.reboot"),
    SessionAction::PowerOff => t!("app.lock.busy.shutdown"),
  }
  .into()
}

fn icon(action: SessionAction) -> IconName {
  match action {
    SessionAction::Logout => IconName::LogOut,
    SessionAction::Suspend => IconName::Moon,
    SessionAction::Hibernate => IconName::Snowflake,
    SessionAction::SuspendThenHibernate => IconName::Hourglass,
    SessionAction::Reboot | SessionAction::RebootToFirmware => IconName::RotateCw,
    SessionAction::PowerOff => IconName::Power,
  }
}

/// Whether `action` shows, and whether it can be picked. Hibernating hides when
/// the machine can't, the rest only disable. Nothing runs before the
/// capabilities loaded, except logging out
fn availability(action: SessionAction, capabilities: Option<&SessionCapabilities>) -> (bool, bool) {
  use SessionAction as A;
  let Some(c) = capabilities else {
    let hidden = matches!(action, A::Hibernate | A::SuspendThenHibernate);
    return (!hidden, action == A::Logout);
  };
  match action {
    A::Logout => (true, true),
    A::Suspend => (c.suspend, true),
    A::Hibernate => (c.hibernate, true),
    A::SuspendThenHibernate => (c.suspend_then_hibernate, true),
    A::Reboot => (true, c.reboot),
    A::PowerOff => (true, c.power_off),
    A::RebootToFirmware => (c.reboot_to_firmware, true),
  }
}

#[derive(Clone, Debug, PartialEq)]
enum Log {
  Idle,
  Busy(SharedString),
  Error(SharedString),
}

pub struct User {
  pub name: SharedString,
  pub avatar: Option<ImageSource>,
}

/// Checks `user`'s password: `Ok(false)` when wrong, `Err` when the check itself
/// failed. Does whatever comes after a right password too
pub type Check = Rc<dyn Fn(String, String, &mut App) -> Task<Result<bool>>>;

/// The user, password field, log line and session menu over the lock and login
/// backdrop
pub struct AuthScreen {
  purpose: Purpose,
  user: User,
  check: Check,
  input: Entity<InputState>,
  log: Log,
  invalid: bool,
  /// A password check runs, the field takes no input until it answered
  checking: bool,
  capabilities: Option<SessionCapabilities>,
  _subscription: Subscription,
}

impl AuthScreen {
  pub fn new(
    purpose: Purpose,
    user: User,
    check: Check,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let input = cx.new(|cx| InputState::new(window, cx).masked(true));
    let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
      InputEvent::PressEnter { .. } => this.submit(window, cx),
      // typing clears an auth error
      InputEvent::Change if matches!(this.log, Log::Error(_)) || this.invalid => {
        this.log = Log::Idle;
        this.invalid = false;
        cx.notify();
      }
      _ => {}
    });
    Self {
      purpose,
      user,
      check,
      input,
      log: Log::Idle,
      invalid: false,
      checking: false,
      capabilities: None,
      _subscription: subscription,
    }
  }

  pub fn focus_handle(&self, cx: &App) -> FocusHandle {
    self.input.read(cx).focus_handle(cx)
  }

  fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    // one check at a time, and none while a session action runs
    if matches!(self.log, Log::Busy(_)) {
      return;
    }
    let password = self.input.read(cx).value().to_string();
    if password.is_empty() {
      self.log = Log::Error(t!("app.lock.empty").into());
      cx.notify();
      return;
    }
    self.log = Log::Busy(t!("app.lock.checking").into());
    self.set_checking(true, window, cx);
    let check = (self.check)(self.user.name.to_string(), password, cx);
    cx.spawn_in(window, async move |this, cx| {
      let result = check.await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.log = Log::Idle;
        this.set_checking(false, window, cx);
        match result {
          Ok(true) => {}
          Ok(false) => {
            this.invalid = true;
            this.log = Log::Error(t!("app.lock.wrong").into());
            this
              .input
              .update(cx, |input, cx| input.set_value("", window, cx));
          }
          Err(e) => {
            tracing::warn!("auth: password check failed: {e:#}");
            this.log = Log::Error(t!("app.lock.auth_failed").into());
          }
        }
        cx.notify();
      });
    })
    .detach();
  }

  fn set_checking(&mut self, checking: bool, window: &mut Window, cx: &mut Context<Self>) {
    self.checking = checking;
    // right away, not on the next render, so no key slips in
    self
      .input
      .update(cx, |input, cx| input.set_disabled(checking, cx));
    if !checking {
      // disabling dropped the focus
      self.input.read(cx).focus_handle(cx).focus(window, cx);
    }
    cx.notify();
  }

  /// Reads what the machine allows on every open
  fn load_capabilities(&mut self, cx: &mut Context<Self>) {
    let capabilities = cx.power().session_capabilities();
    cx.spawn(async move |this, cx| {
      let capabilities = capabilities.await.log_err().unwrap_or_default();
      let _ = this.update(cx, |this, cx| {
        this.capabilities = Some(capabilities);
        cx.notify();
      });
    })
    .detach();
  }

  fn activate(&mut self, action: SessionAction, cx: &mut Context<Self>) {
    self.log = Log::Busy(busy(action));
    cx.notify();
    let task = cx.power().session_action(action);
    cx.spawn(async move |this, cx| {
      let result = task.await;
      let _ = this.update(cx, |this, cx| {
        this.log = match result {
          Err(e) => {
            tracing::warn!("auth: {action:?} failed: {e:#}");
            Log::Error(t!("app.lock.failed", action = label(action)).into())
          }
          // back from sleep, or the request only got queued
          Ok(()) => Log::Idle,
        };
        cx.notify();
      });
    })
    .detach();
  }

  fn header(&self, cx: &App) -> impl IntoElement + use<> {
    div()
      .flex()
      .flex_col()
      .items_center()
      .gap(px(14.))
      .child(
        Avatar::new()
          .name(self.user.name.clone())
          .when_some(self.user.avatar.clone(), |a, src| a.src(src))
          .with_size(px(96.))
          .text_size(px(30.))
          .font_weight(gpui_kit::FontWeight::MEDIUM),
      )
      .child(
        div()
          .text_size(px(20.))
          .line_height(px(28.))
          .font_weight(gpui_kit::FontWeight::MEDIUM)
          .text_color(cx.theme().foreground)
          .child(self.user.name.clone()),
      )
  }

  fn password(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
    let submit = InputGroupButton::new("auth-submit")
      .icon(IconName::ArrowRight)
      .accessibility_label(self.purpose.submit())
      .cursor_pointer()
      .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx)));

    field().label(t!("app.lock.password").to_string()).child(
      InputGroup::new("auth-password")
        .input(
          gpui_kit::component::input::Input::new(&self.input)
            .mask_toggle()
            .disabled(self.checking),
        )
        .disabled(self.checking)
        .invalid(self.invalid)
        .addon(
          InputGroupAddon::new("auth-actions")
            .align(InputGroupAddonAlignment::InlineEnd)
            .child(submit),
        ),
    )
  }

  fn log_line(&self, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let (text, color, busy): (SharedString, Hsla, bool) = match &self.log {
      Log::Idle => (self.purpose.idle(), theme.muted_foreground, false),
      Log::Busy(text) => (text.clone(), theme.muted_foreground, true),
      Log::Error(text) => (text.clone(), theme.danger, false),
    };
    div()
      .min_h(px(20.))
      .flex()
      .items_center()
      .justify_center()
      .gap_2()
      .text_size(px(12.))
      .font_family(theme.mono_font_family.clone())
      .text_color(color)
      .when(busy, |d| {
        d.child(Spinner::new().with_size(px(12.)).color(color))
      })
      .child(div().truncate().child(text))
  }

  fn menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
    let capabilities = self.capabilities.as_ref();
    let this = cx.entity();
    let groups = self
      .purpose
      .groups()
      .into_iter()
      .map(|group| {
        group
          .into_iter()
          .filter_map(|action| {
            let (shown, enabled) = availability(action, capabilities);
            let this = this.clone();
            shown.then(|| MenuItem {
              id: format!("auth-menu-{action:?}").into(),
              icon: Some(icon(action)),
              label: label(action),
              enabled,
              on_click: Rc::new(move |_, cx| this.update(cx, |this, cx| this.activate(action, cx))),
            })
          })
          .collect()
      })
      .collect();
    let trigger = Button::new("auth-menu-trigger")
      .icon(IconName::Power)
      .tooltip(self.purpose.menu());
    corner_menu(
      "auth-menu",
      Anchor::BottomRight,
      trigger,
      groups,
      move |cx| this.update(cx, |this, cx| this.load_capabilities(cx)),
    )
  }
}

pub struct MenuItem {
  pub id: SharedString,
  pub icon: Option<IconName>,
  pub label: SharedString,
  pub enabled: bool,
  pub on_click: OnClick,
}

pub type OnClick = Rc<dyn Fn(&mut Window, &mut App)>;

/// `trigger` in the `anchor` corner, opening `groups` of items, a line between
/// each. `on_open` runs every time it opens
pub fn corner_menu(
  id: &'static str,
  anchor: Anchor,
  trigger: Button,
  groups: Vec<Vec<MenuItem>>,
  on_open: impl Fn(&mut App) + 'static,
) -> impl IntoElement {
  let groups: Vec<_> = groups.into_iter().filter(|g| !g.is_empty()).collect();
  div()
    .absolute()
    .bottom(px(INSET))
    .map(|d| match anchor {
      Anchor::BottomLeft => d.left(px(INSET)),
      _ => d.right(px(INSET)),
    })
    .child(
      Popover::new(id)
        .anchor(anchor)
        .offset(px(8.))
        .p_1()
        .rounded_xl()
        .on_open_change(move |open, _, cx| {
          if *open {
            on_open(cx);
          }
        })
        .trigger(trigger.cursor_pointer())
        .content(move |_, _, cx| {
          let hover = cx.theme().accent;
          let popover = cx.entity();
          let mut panel = div().w(px(MENU_WIDTH)).flex().flex_col();
          for (i, group) in groups.iter().enumerate() {
            if i > 0 {
              panel = panel.child(Separator::horizontal().my_1());
            }
            panel = panel.children(group.iter().map(|item| {
              let (on_click, popover) = (item.on_click.clone(), popover.clone());
              div()
                .id(item.id.clone())
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_1p5()
                .rounded_md()
                .child(match item.icon {
                  Some(icon) => Icon::new(icon).size_4().into_any_element(),
                  None => div().size_4().into_any_element(),
                })
                .child(item.label.clone())
                .when(!item.enabled, |d| d.opacity(0.5))
                .when(item.enabled, |d| {
                  d.cursor_pointer()
                    .hover(move |d| d.bg(hover))
                    .on_click(move |_, window, cx| {
                      popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                      on_click(window, cx);
                    })
                })
            }));
          }
          panel
        }),
    )
}

impl Render for AuthScreen {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .relative()
      .text_sm()
      .child(
        div()
          .absolute()
          .size_full()
          .flex()
          .items_center()
          .justify_center()
          .child(
            div()
              .w(px(WIDTH))
              .flex()
              .flex_col()
              .gap_4()
              .child(self.header(cx))
              .child(
                div()
                  .flex()
                  .flex_col()
                  .gap(px(14.))
                  .child(self.password(cx))
                  .child(self.log_line(cx)),
              ),
          ),
      )
      .child(self.menu(cx))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hibernating_hides_until_supported() {
    use SessionAction as A;
    for action in [A::Hibernate, A::SuspendThenHibernate] {
      assert_eq!(availability(action, None), (false, false));
      let none = SessionCapabilities::default();
      assert!(!availability(action, Some(&none)).0);
    }
    // suspending hides too once known unsupported
    let none = SessionCapabilities::default();
    assert!(!availability(A::Suspend, Some(&none)).0);
    assert_eq!(availability(A::Suspend, None), (true, false));
    // the rest show disabled
    for action in [A::Reboot, A::PowerOff] {
      assert_eq!(availability(action, Some(&none)), (true, false));
      assert_eq!(availability(action, None), (true, false));
    }
    assert_eq!(availability(A::Logout, None), (true, true));
  }

  #[test]
  fn unlock_menu_groups() {
    let groups = Purpose::Unlock.groups();
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0], [SessionAction::Logout]);
    assert_eq!(groups[2].last(), Some(&SessionAction::PowerOff));
  }

  #[test]
  fn login_menu_has_no_logout() {
    let groups = Purpose::Login.groups();
    assert!(groups.iter().flatten().all(|&a| a != SessionAction::Logout));
    assert_eq!(
      groups.last().unwrap().last(),
      Some(&SessionAction::PowerOff)
    );
  }
}
