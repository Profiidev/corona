use corona_config::ConfigProvider;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Context, Entity, Global, IntoElement, ParentElement, Render, Styled, WeakEntity,
  Window, div,
};

use crate::{
  control_center::{
    ControlCenterExt, ControlCenterPage, ControlCenterPageHandle, layout::ControlCenterLayout,
    nav::ControlCenterNav,
  },
  panel::{Panel, PanelState},
};

pub struct ControlCenter {
  selected: &'static str,
  panel: Option<Box<dyn ControlCenterPageHandle>>,
}

struct OpenControlCenter(WeakEntity<ControlCenter>);

impl Global for OpenControlCenter {}

impl ControlCenter {
  /// Shows `P` in the open control center, else as its own panel
  pub fn navigate<P: ControlCenterPage>(window: &mut Window, cx: &mut App) {
    Self::navigate_id(P::ID, window, cx)
  }

  /// [`Self::navigate`] for a page id, like one from the config
  pub fn navigate_id(page: &str, window: &mut Window, cx: &mut App) {
    let open = cx
      .try_global::<OpenControlCenter>()
      .and_then(|open| open.0.upgrade());
    match open {
      Some(control_center) => control_center.update(cx, |this, cx| this.select(page, window, cx)),
      None => {
        let _ = PanelState::show(page, false, cx).log_err();
      }
    }
  }

  fn select(&mut self, page: &str, window: &mut Window, cx: &mut Context<Self>) {
    let Some(entry) = cx.control_center().get(page) else {
      tracing::warn!("no control center page {page:?}");
      return;
    };
    let (id, open) = (entry.id, entry.open);
    self.selected = id;
    self.panel = Some(open(window, cx));
    cx.notify();
  }
}

impl Panel for ControlCenter {
  const NAME: &'static str = "control_center";
  const WIDTH: f32 = 500.0;
  const HEIGHT: f32 = 500.0;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let this = cx.entity().downgrade();
    cx.set_global(OpenControlCenter(this));

    let first = (cx.control_center())
      .sidebar(&cx.config().control_center)
      .first()
      .map(|p| (p.id, p.open));
    ControlCenter {
      selected: first.map_or("", |(id, _)| id),
      panel: first.map(|(_, open)| open(window, cx)),
    }
  }
}

/// A page as its own panel, beside the control center
pub struct Standalone<T: ControlCenterPage>(Entity<T>);

impl<T: ControlCenterPage> Panel for Standalone<T> {
  const NAME: &'static str = T::ID;
  const WIDTH: f32 = ControlCenter::WIDTH - 8.0 - ControlCenterNav::WIDTH;
  const HEIGHT: f32 = <T as ControlCenterPage>::HEIGHT;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    Self(cx.new(|cx| T::init(window, cx)))
  }
}

impl<T: ControlCenterPage> Render for Standalone<T> {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div().size_full().p_2().child(
      ControlCenterLayout::new(T::ID)
        .closes::<Self>()
        .buttons(self.0.buttons(cx).into_iter())
        .content(self.0.clone()),
    )
  }
}

impl Render for ControlCenter {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .size_full()
      .flex()
      .gap_2()
      .p_2()
      .child(ControlCenterNav::new(self.selected).on_click({
        let handle = cx.entity().downgrade();
        move |new, window, cx| {
          let _ = handle.update(cx, |this, cx| this.select(new, window, cx));
        }
      }))
      .child(match &self.panel {
        Some(panel) => ControlCenterLayout::new(self.selected)
          .buttons(panel.buttons(cx).into_iter())
          .content(panel.view()),
        None => ControlCenterLayout::new(self.selected),
      })
  }
}
