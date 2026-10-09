use anyhow::Result;
use gpui_kit::App;

pub mod bar;
pub mod commands;
pub mod input_region;
pub mod osd;
pub mod panel;
pub mod per_display;
pub mod popup;
pub mod tooltip;

pub fn init(cx: &mut App) -> Result<()> {
  bar::BarState::init(cx);
  panel::PanelState::init(cx);
  tooltip::TooltipState::init(cx);
  osd::OsdState::init(cx);
  Ok(())
}

#[cfg(test)]
pub(crate) mod test_support {
  use std::{cell::RefCell, rc::Rc};

  use anyhow::Result;
  use corona_compositor::{Compositor, CompositorImpl, types};
  use gpui_kit::{
    AnyWindowHandle, App, AppContext, Context, IntoElement, ParentElement, Pixels, Render, Size,
    Styled, TestAppContext, Window, WindowOptions, div, px, size,
  };
  use uuid::Uuid;

  use crate::{bar::Widget, osd::Osd, panel::Panel, tooltip::Tooltip};

  fn workspace() -> types::Workspace {
    types::Workspace {
      id: "1".into(),
      name: "1".into(),
      monitor: "DP-1".into(),
      monitor_id: 0,
    }
  }

  pub fn monitor(name: &str) -> types::Monitor {
    types::Monitor {
      id: 0,
      name: name.into(),
      width: 1920,
      height: 1080,
      refresh_rate: 60.,
      x: 0,
      y: 0,
      active_scratchpad: None,
      active_workspace: workspace(),
      scale: 1.,
      focused: true,
      disabled: false,
      mirror_of: "none".into(),
    }
  }

  /// A compositor with fixed monitors and nothing else
  #[derive(Default)]
  pub struct Fake {
    pub monitors: RefCell<Vec<types::Monitor>>,
  }

  impl CompositorImpl for Fake {
    fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
      Ok(vec![workspace()])
    }
    fn active_workspace(&self) -> Result<types::Workspace> {
      Ok(workspace())
    }
    fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
      Ok(self.monitors.borrow().clone())
    }
    fn active_monitor(&self) -> Result<types::Monitor> {
      Ok(monitor("DP-1"))
    }
    fn list_windows(&self) -> Result<Vec<types::Window>> {
      Ok(vec![])
    }
    fn active_window(&self) -> Result<Option<types::Window>> {
      Ok(None)
    }
    fn focus_workspace(&self, _: &str) -> Result<()> {
      Ok(())
    }
    fn focus_window(&self, _: &str) -> Result<()> {
      Ok(())
    }
    fn close_window(&self, _: &str) -> Result<()> {
      Ok(())
    }
    fn cursor_position(&self) -> Result<(i32, i32)> {
      Ok((0, 0))
    }
    fn keyboard_layout(&self) -> Result<Option<String>> {
      Ok(None)
    }
    fn set_dpms(&self, _: bool) -> Result<()> {
      Ok(())
    }
    fn configure_monitor(&self, _: &str, _: types::MonitorChange) -> Result<()> {
      Ok(())
    }
  }

  /// Kit, default config, a fake compositor and every surface global
  pub fn setup(cx: &mut TestAppContext) -> Rc<Fake> {
    let fake = Rc::new(Fake::default());
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
      crate::init(cx).unwrap();
    });
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
    const NAME: &'static str = "osd_a";
    fn size(&self, _: &App) -> Size<Pixels> {
      size(px(100.), px(30.))
    }
  }

  impl Osd for OsdB {
    const NAME: &'static str = "osd_b";
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
