//! The shared OSD views, each kind wraps one, and the watcher that shows them

use std::{borrow::Cow, time::Duration};

use gpui_kit::{
  App, Div, Entity, ParentElement, Pixels, Size, Styled,
  assets::IconName,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
  px, relative, size,
};
use rust_i18n::t;

const ICON: f32 = 20.;

pub struct LevelOsd {
  pub icon: IconName,
  pub label: Cow<'static, str>,
  pub percent: f32,
  pub muted: bool,
}

impl LevelOsd {
  pub fn size(&self) -> Size<Pixels> {
    size(px(300.), px(56.))
  }

  pub fn view(&self, cx: &App) -> Div {
    let theme = cx.theme();
    let fill = if self.muted {
      theme.muted_foreground
    } else {
      theme.primary
    };
    div()
      .size_full()
      .flex()
      .items_center()
      .gap_3()
      .px_4()
      .child(Icon::new(self.icon).with_size(px(ICON)))
      .child(
        div()
          .flex_1()
          .flex()
          .flex_col()
          .gap_1()
          .child(
            div()
              .text_xs()
              .text_color(theme.muted_foreground)
              .truncate()
              .child(self.label.clone()),
          )
          .child(
            div()
              .relative()
              .h(px(6.))
              .w_full()
              .rounded_full()
              .bg(theme.muted)
              .child(
                div()
                  .absolute()
                  .left_0()
                  .h_full()
                  .w(relative((self.percent / 100.).clamp(0., 1.)))
                  .rounded_full()
                  .bg(fill),
              ),
          ),
      )
      .child(
        div()
          .flex_none()
          .min_w(px(40.))
          .whitespace_nowrap()
          .text_sm()
          .text_right()
          .child(format!("{:.0}%", self.percent)),
      )
  }
}

pub struct ToggleOsd {
  pub icon: IconName,
  pub label: Cow<'static, str>,
  pub state: Cow<'static, str>,
  pub active: bool,
}

impl ToggleOsd {
  pub fn on_off(icon: IconName, label: Cow<'static, str>, on: bool) -> Self {
    Self {
      icon,
      label,
      state: if on {
        t!("app.common.on")
      } else {
        t!("app.common.off")
      },
      active: on,
    }
  }
}

impl ToggleOsd {
  pub fn size(&self) -> Size<Pixels> {
    size(px(300.), px(48.))
  }

  pub fn view(&self, cx: &App) -> Div {
    let theme = cx.theme();
    div()
      .size_full()
      .flex()
      .items_center()
      .gap_3()
      .px_4()
      .child(
        Icon::new(self.icon)
          .with_size(px(ICON))
          .when(self.active, |icon| icon.text_color(theme.primary)),
      )
      .child(
        div()
          .flex_1()
          .min_w_0()
          .truncate()
          .text_sm()
          .child(self.label.clone()),
      )
      .child(
        div()
          .text_sm()
          .text_color(theme.muted_foreground)
          .truncate()
          .child(self.state.clone()),
      )
  }
}

/// Changes to backends settle at startup, they are not shown before this
const SETTLE: Duration = Duration::from_secs(3);

/// Calls `show` with the value `read` had and has whenever it changes, once the
/// startup has settled. Appearing or vanishing (`None`) is not a change.
pub fn on_change<E: 'static, K: PartialEq + 'static>(
  entity: &Entity<E>,
  cx: &mut App,
  read: impl Fn(&App) -> Option<K> + 'static,
  show: impl Fn(&K, &K, &mut App) + 'static,
) {
  let entity = entity.clone();
  cx.spawn(async move |cx| {
    cx.background_executor().timer(SETTLE).await;
    cx.update(|cx| {
      let mut last = read(cx);
      cx.observe(&entity, move |_, cx| {
        let next = read(cx);
        if next == last {
          return;
        }
        if let (Some(prev), Some(next)) = (&last, &next) {
          show(prev, next, cx);
        }
        last = next;
      })
      .detach();
    });
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn on_off_state() {
    let on = ToggleOsd::on_off(IconName::Wifi, "Wi-Fi".into(), true);
    let off = ToggleOsd::on_off(IconName::Wifi, "Wi-Fi".into(), false);
    assert!(on.active && !off.active);
    assert_ne!(on.state, off.state);
    assert!(!on.state.is_empty() && !on.state.starts_with("app."));
  }

  use std::{cell::RefCell, rc::Rc};

  use gpui_kit::{
    self as gpui, AppContext as _, Context, IntoElement, Render, TestAppContext, Window,
    WindowOptions, test::TestWindowExt,
  };

  struct View(Box<dyn Fn(&App) -> Div>);

  impl Render for View {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
      (self.0)(cx)
    }
  }

  fn draw(cx: &mut TestAppContext, view: impl Fn(&App) -> Div + 'static) {
    let handle = cx.update(|cx| {
      cx.open_window(WindowOptions::default(), |_, cx| {
        cx.new(|_| View(Box::new(view)))
      })
      .unwrap()
    });
    cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx))
      .unwrap();
  }

  #[gpui::test]
  fn osds_render(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
    });
    for (percent, muted) in [(-10., false), (50., true), (150., false)] {
      let level = LevelOsd {
        icon: IconName::Volume2,
        label: "Volume".into(),
        percent,
        muted,
      };
      draw(cx, move |cx| level.view(cx));
    }
    let toggle = ToggleOsd::on_off(IconName::Wifi, "Wi-Fi".into(), true);
    draw(cx, move |cx| toggle.view(cx));
    let level = LevelOsd {
      icon: IconName::Sun,
      label: "".into(),
      percent: 0.,
      muted: false,
    };
    let toggle = ToggleOsd::on_off(IconName::Wifi, "".into(), false);
    assert!(level.size().width > px(0.) && toggle.size().height > px(0.));
  }

  type Shown = Rc<RefCell<Vec<(i32, i32)>>>;

  fn watch(cx: &mut TestAppContext, start: Option<i32>) -> (Entity<Option<i32>>, Shown) {
    let shown = Shown::default();
    let entity = cx.new(|_| start);
    let (e, s) = (entity.clone(), shown.clone());
    cx.update(|cx| {
      on_change(
        &e.clone(),
        cx,
        move |cx| *e.read(cx),
        move |prev, next, _| s.borrow_mut().push((*prev, *next)),
      )
    });
    cx.executor().advance_clock(SETTLE);
    cx.run_until_parked();
    (entity, shown)
  }

  fn set(entity: &Entity<Option<i32>>, value: Option<i32>, cx: &mut TestAppContext) {
    entity.update(cx, |v, cx| {
      *v = value;
      cx.notify();
    });
    cx.run_until_parked();
  }

  #[gpui::test]
  fn shows_changes_only(cx: &mut TestAppContext) {
    let (entity, shown) = watch(cx, Some(1));
    set(&entity, Some(1), cx);
    assert!(shown.borrow().is_empty());
    set(&entity, Some(2), cx);
    set(&entity, Some(5), cx);
    assert_eq!(*shown.borrow(), [(1, 2), (2, 5)]);
  }

  #[gpui::test]
  fn appearing_or_vanishing_is_not_shown(cx: &mut TestAppContext) {
    let (entity, shown) = watch(cx, None);
    set(&entity, Some(3), cx);
    set(&entity, None, cx);
    assert!(shown.borrow().is_empty());
    // the last value was still kept
    set(&entity, Some(4), cx);
    set(&entity, Some(6), cx);
    assert_eq!(*shown.borrow(), [(4, 6)]);
  }

  #[gpui::test]
  fn changes_while_settling_are_not_shown(cx: &mut TestAppContext) {
    let shown = Shown::default();
    let entity = cx.new(|_| Some(1));
    let (e, s) = (entity.clone(), shown.clone());
    cx.update(|cx| {
      on_change(
        &e.clone(),
        cx,
        move |cx| *e.read(cx),
        move |prev, next, _| s.borrow_mut().push((*prev, *next)),
      )
    });
    set(&entity, Some(2), cx);
    cx.executor().advance_clock(SETTLE);
    cx.run_until_parked();
    assert!(shown.borrow().is_empty());
    set(&entity, Some(3), cx);
    assert_eq!(*shown.borrow(), [(2, 3)]);
  }
}
