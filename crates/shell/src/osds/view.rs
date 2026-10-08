use corona_config::{ConfigProvider, OsdKinds};
use corona_surface::osd::{Osd, OsdExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, Context, IntoElement, ParentElement, Pixels, Render, Size, Styled, Window,
  assets::IconName,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
  px, relative, size,
};
use rust_i18n::t;
use std::borrow::Cow;

const ICON: f32 = 20.;

pub struct LevelOsd {
  pub icon: IconName,
  pub label: Cow<'static, str>,
  pub percent: f32,
  pub muted: bool,
}

impl Render for LevelOsd {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

impl Osd for LevelOsd {
  const NAME: &'static str = "level";

  fn size(&self, _cx: &App) -> Size<Pixels> {
    size(px(300.), px(56.))
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

impl Render for ToggleOsd {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

impl Osd for ToggleOsd {
  const NAME: &'static str = "toggle";

  fn size(&self, _cx: &App) -> Size<Pixels> {
    size(px(300.), px(48.))
  }
}

/// Shows `osd` unless the OSD, or its `kind`, is turned off in the settings.
pub fn show(kind: fn(&OsdKinds) -> bool, osd: impl Osd, cx: &mut App) {
  let config = &cx.config().osd;
  if config.enabled && kind(&config.kinds) {
    let _ = cx.show_osd(osd).log_err();
  }
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

  use gpui_kit::{
    self as gpui, AppContext as _, TestAppContext, WindowOptions, test::TestWindowExt,
  };

  #[gpui::test]
  fn osds_render(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
    });
    for (percent, muted) in [(-10., false), (50., true), (150., false)] {
      let handle = cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |_, cx| {
          cx.new(|_| LevelOsd {
            icon: IconName::Volume2,
            label: "Volume".into(),
            percent,
            muted,
          })
        })
        .unwrap()
      });
      cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx))
        .unwrap();
    }
    let handle = cx.update(|cx| {
      cx.open_window(WindowOptions::default(), |_, cx| {
        cx.new(|_| ToggleOsd::on_off(IconName::Wifi, "Wi-Fi".into(), true))
      })
      .unwrap()
    });
    cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx))
      .unwrap();
    let sizes = cx.update(|cx| {
      let level = LevelOsd {
        icon: IconName::Sun,
        label: "".into(),
        percent: 0.,
        muted: false,
      };
      let toggle = ToggleOsd::on_off(IconName::Wifi, "".into(), false);
      (level.size(cx), toggle.size(cx))
    });
    assert!(sizes.0.width > px(0.) && sizes.1.height > px(0.));
  }
}
