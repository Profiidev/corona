rust_i18n::i18n!("../../assets/locales", fallback = "en");

use anyhow::Result;
use gpui_kit::App;

use crate::{
  bar::BarExt, control_center::ControlCenterExt, dashboard::DashboardExt, panel::AppPanelExt,
};

pub mod bar;
pub mod cli;
pub mod commands;
pub mod control_center;
pub mod dashboard;
pub mod input_region;
pub mod osd;
pub mod overlay;
pub mod panel;
pub mod per_display;
pub mod popup;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
pub mod theme;
pub mod tooltip;

pub fn init(cx: &mut App) -> Result<()> {
  bar::BarState::init(cx);
  panel::PanelState::init(cx);
  tooltip::TooltipState::init(cx);
  osd::OsdState::init(cx);
  commands::register_commands(cx);
  theme::register_commands(cx);
  Ok(())
}

/// The control center with its dashboard, its bar button and the dark mode tile; before
/// the integrations register theirs
pub fn register_builtins(cx: &mut App) {
  cx.bar_mut()
    .register::<control_center::ControlCenterButton>();
  cx.panel().register::<control_center::ControlCenter>();
  cx.control_center_mut()
    .register::<dashboard::DashboardPanel>();
  cx.dashboard_mut().quick_action::<dashboard::DarkMode>();
}

#[cfg(test)]
pub(crate) mod test_support {
  use std::rc::Rc;

  use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, IntoElement, ParentElement, Pixels, Render, Size,
    Styled, TestAppContext, Window, WindowOptions, div, px, size,
  };
  use uuid::Uuid;

  use crate::{bar::Widget, osd::Osd, panel::Panel, testing::FakeCompositor, tooltip::Tooltip};

  /// Kit, default config, a fake compositor and every surface global
  pub fn setup(cx: &mut TestAppContext) -> Rc<FakeCompositor> {
    let fake = crate::testing::setup(FakeCompositor::default(), cx);
    cx.update(|cx| crate::init(cx).unwrap());
    fake
  }

  pub fn windows(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| cx.windows().len())
  }

  /// Draws every window once, then runs what that spawned
  pub fn draw_all(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    for handle in cx.update(|cx| cx.windows()) {
      let _ = cx.update_window(handle, |_, window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
  }

  /// No animations, so panels close within one frame
  pub fn no_animations(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.global_mut::<corona_config::Config>()
        .shell
        .animation
        .enabled = false
    });
  }

  pub struct Empty;

  impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div().size_full()
    }
  }

  /// A plain window to parent popups on
  pub fn plain_window(cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.update(|cx| {
      cx.open_window(WindowOptions::default(), |_, cx| cx.new(|_| Empty))
        .unwrap()
        .into()
    })
  }

  macro_rules! dummy {
    ($name:ident) => {
      pub struct $name;

      impl Render for $name {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
          div().size(px(4.))
        }
      }
    };
  }

  dummy!(PanelA);
  dummy!(PanelB);
  dummy!(TipB);
  dummy!(OsdA);
  dummy!(OsdB);

  impl Panel for PanelA {
    const NAME: &'static str = "panel_a";
    const WIDTH: f32 = 200.;
    const HEIGHT: f32 = 100.;
    fn init(_: &mut Window, _: &mut Context<'_, Self>) -> Self {
      Self
    }
  }

  impl Panel for PanelB {
    const NAME: &'static str = "panel_b";
    const WIDTH: f32 = 50.;
    const HEIGHT: f32 = 50.;
    fn init(_: &mut Window, _: &mut Context<'_, Self>) -> Self {
      Self
    }
  }

  /// One tooltip, of any width
  pub struct TipA<const W: u32>;

  impl<const W: u32> Render for TipA<W> {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div().size(px(4.))
    }
  }

  impl<const W: u32> Tooltip for TipA<W> {
    const NAME: &'static str = "tip_a";
    fn size(&self, _: &Window, _: &App) -> Size<Pixels> {
      size(px(W as f32), px(20.))
    }
  }

  impl Tooltip for TipB {
    const NAME: &'static str = "tip_b";
    fn size(&self, _: &Window, _: &App) -> Size<Pixels> {
      size(px(60.), px(20.))
    }
  }

  impl Osd for OsdA {
    const KIND: &'static str = "osd_a";
    fn title() -> gpui_kit::SharedString {
      "OsdA".into()
    }
    fn description() -> gpui_kit::SharedString {
      "".into()
    }
    fn size(&self, _: &App) -> Size<Pixels> {
      size(px(100.), px(30.))
    }
  }

  impl Osd for OsdB {
    const KIND: &'static str = "osd_b";
    fn title() -> gpui_kit::SharedString {
      "OsdB".into()
    }
    fn description() -> gpui_kit::SharedString {
      "".into()
    }
    fn size(&self, _: &App) -> Size<Pixels> {
      size(px(200.), px(30.))
    }
  }

  /// A bar widget that records the options it was built with
  pub struct Label(pub LabelOptions);

  #[derive(serde::Deserialize, Default, Clone, PartialEq, Debug)]
  #[serde(default)]
  pub struct LabelOptions {
    pub text: String,
  }

  impl Render for Label {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
      use crate::bar::BarStyle;
      div()
        .size(px(10.))
        .bar_pill(window, cx)
        .child(self.0.text.clone())
    }
  }

  /// A bar widget that is a panel button
  pub struct Toggle {
    pub danger: bool,
  }

  impl Render for Toggle {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
      crate::bar::Button::<Self, PanelA>::new(cx, "toggle", gpui_kit::assets::IconName::X)
        .danger(self.danger)
        .dot(self.danger)
        .suffix(div().child("1"))
    }
  }

  impl Widget for Toggle {
    const NAME: &'static str = "toggle";
    type Options = bool;
    fn init(_: &mut Context<'_, Self>, _: Uuid, danger: bool) -> Self {
      Self { danger }
    }
  }

  impl Widget for Label {
    const NAME: &'static str = "label";
    type Options = LabelOptions;
    fn init(_: &mut Context<'_, Self>, _: Uuid, options: LabelOptions) -> Self {
      Self(options)
    }
  }
}
