use std::time::Duration;

use anyhow::Result;
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider, OsdPosition};
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyView, AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Global, IntoElement,
  ParentElement, Pixels, Point, Render, Size, Styled, Task, WeakEntity, Window,
  WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  px,
};

const NAMESPACE: &str = "corona_osd";
const BORDER: f32 = 2.;

pub trait Osd: Render {
  const NAME: &'static str;

  fn size(&self, cx: &App) -> Size<Pixels>;
}

struct Shown {
  name: &'static str,
  display: Option<DisplayId>,
  size: Size<Pixels>,
  handle: AnyWindowHandle,
  view: WeakEntity<BaseOsd>,
  _hide: Task<()>,
}

#[derive(Default)]
pub struct OsdState {
  shown: Option<Shown>,
}

impl Global for OsdState {}

impl OsdState {
  pub fn init(cx: &mut App) {
    cx.set_global(OsdState::default());
  }

  pub fn show<T: Osd>(osd: T, cx: &mut App) -> Result<()> {
    let monitor = cx.compositor().active_monitor(cx).name.clone();
    let display = display_id_for(&monitor, cx);
    let content = osd.size(cx);
    let border = cx.config().theme.popup_border(BORDER);
    let size = Size::new(
      content.width + px(border * 2.),
      content.height + px(border * 2.),
    );
    let timeout = Duration::from_millis(cx.config().osd.hide_delay_ms);
    let view: AnyView = cx.new(|_| osd).into();

    let reused = cx
      .global_mut::<OsdState>()
      .shown
      .take_if(|s| s.display == display)
      .and_then(|shown| Some((shown.handle, shown.view.upgrade()?, shown.size)));
    let (handle, base) = match reused {
      Some((handle, base, old_size)) => {
        base.update(cx, |base, cx| {
          base.content = view;
          cx.notify();
        });
        if old_size != size {
          let _ = handle.update(cx, |_, window, _| window.resize(size));
        }
        (handle, base.downgrade())
      }
      None => {
        Self::hide(cx);
        Self::open(view, size, display, cx)?
      }
    };

    let hide = cx.spawn(async move |cx| {
      cx.background_executor().timer(timeout).await;
      cx.update(Self::hide);
    });
    cx.global_mut::<OsdState>().shown = Some(Shown {
      name: T::NAME,
      display,
      size,
      handle,
      view: base,
      _hide: hide,
    });
    Ok(())
  }

  fn open(
    content: AnyView,
    size: Size<Pixels>,
    display: Option<DisplayId>,
    cx: &mut App,
  ) -> Result<(AnyWindowHandle, WeakEntity<BaseOsd>)> {
    let mut base = WeakEntity::new_invalid();
    let config = &cx.config().osd;
    let (anchor, margin) = edge(config.position, px(config.offset));
    let handle = cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: Some(margin),
          layer: Layer::Overlay,
          namespace: NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::None,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: Point::default(),
          size,
        })),
        display_id: display,
        ..Default::default()
      },
      |window, cx| {
        window.set_input_region(Some(&[]));
        let view = cx.new(|_| BaseOsd { content });
        base = view.downgrade();
        cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
      },
    )?;
    Ok((handle.into(), base))
  }

  pub fn hide(cx: &mut App) {
    let Some(shown) = cx.global_mut::<OsdState>().shown.take() else {
      return;
    };
    let _ = shown
      .handle
      .update(cx, |_, window, _| window.remove_window());
  }

  pub fn hide_osd<T: Osd>(cx: &mut App) {
    if cx
      .global::<OsdState>()
      .shown
      .as_ref()
      .is_some_and(|s| s.name == T::NAME)
    {
      Self::hide(cx);
    }
  }
}

/// The edge an OSD sits on, and its margins: top, right, bottom, left
fn edge(position: OsdPosition, offset: Pixels) -> (Anchor, (Pixels, Pixels, Pixels, Pixels)) {
  let zero = px(0.);
  match position {
    OsdPosition::TopCenter => (Anchor::TOP, (offset, zero, zero, zero)),
    OsdPosition::BottomCenter => (Anchor::BOTTOM, (zero, zero, offset, zero)),
    OsdPosition::CenterLeft => (Anchor::LEFT, (zero, zero, zero, offset)),
    OsdPosition::CenterRight => (Anchor::RIGHT, (zero, offset, zero, zero)),
  }
}

struct BaseOsd {
  content: AnyView,
}

impl Render for BaseOsd {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let config = cx.config();
    div()
      .size_full()
      .bg(
        theme
          .tokens
          .background
          .opacity(config.osd.background_opacity),
      )
      .rounded(theme.radius * 2)
      .border(px(config.theme.popup_border(BORDER)))
      .border_color(theme.tokens.button_hover)
      .overflow_hidden()
      .child(self.content.clone())
  }
}

pub trait OsdExt {
  fn show_osd<T: Osd>(&mut self, osd: T) -> Result<()>;
  fn hide_osd<T: Osd>(&mut self);
}

impl OsdExt for App {
  fn show_osd<T: Osd>(&mut self, osd: T) -> Result<()> {
    OsdState::show(osd, self)
  }

  fn hide_osd<T: Osd>(&mut self) {
    OsdState::hide_osd::<T>(self)
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{TestAppContext, size};

  use super::*;
  use crate::test_support::{self, OsdA, OsdB, draw_all, windows};

  fn delay(cx: &mut TestAppContext) -> Duration {
    cx.update(|cx| Duration::from_millis(cx.config().osd.hide_delay_ms))
  }

  fn shown(cx: &mut TestAppContext) -> Option<(&'static str, AnyWindowHandle, Size<Pixels>)> {
    cx.update(|cx| {
      cx.global::<OsdState>()
        .shown
        .as_ref()
        .map(|s| (s.name, s.handle, s.size))
    })
  }

  #[test]
  fn edge_puts_the_offset_on_the_anchored_side() {
    let o = px(7.);
    let z = px(0.);
    assert_eq!(edge(OsdPosition::TopCenter, o), (Anchor::TOP, (o, z, z, z)));
    assert_eq!(
      edge(OsdPosition::CenterRight, o),
      (Anchor::RIGHT, (z, o, z, z))
    );
    assert_eq!(
      edge(OsdPosition::BottomCenter, o),
      (Anchor::BOTTOM, (z, z, o, z))
    );
    assert_eq!(
      edge(OsdPosition::CenterLeft, o),
      (Anchor::LEFT, (z, z, z, o))
    );
  }

  #[gpui_kit::test]
  fn show_reuses_the_window_and_follows_the_size(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let before = windows(cx);
    cx.update(|cx| cx.show_osd(OsdA).unwrap());
    let (name, handle, size_a) = shown(cx).unwrap();
    assert_eq!(name, OsdA::NAME);
    // the popup border goes around the content
    assert_eq!(size_a, size(px(104.), px(34.)));
    assert_eq!(windows(cx), before + 1);
    draw_all(cx);

    cx.update(|cx| cx.show_osd(OsdB).unwrap());
    let (name, again, size_b) = shown(cx).unwrap();
    assert_eq!(name, OsdB::NAME);
    assert!(again == handle);
    assert_eq!(size_b, size(px(204.), px(34.)));
    assert_eq!(windows(cx), before + 1);
    draw_all(cx);
  }

  #[gpui_kit::test]
  fn without_borders_the_size_is_the_content(cx: &mut TestAppContext) {
    test_support::setup(cx);
    cx.update(|cx| {
      cx.global_mut::<corona_config::Config>().theme.popup_borders = false;
      cx.show_osd(OsdA).unwrap();
    });
    assert_eq!(shown(cx).unwrap().2, size(px(100.), px(30.)));
  }

  #[gpui_kit::test]
  fn hides_after_the_delay_and_showing_restarts_it(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let delay = delay(cx);
    let before = windows(cx);
    cx.update(|cx| cx.show_osd(OsdA).unwrap());
    cx.executor().advance_clock(delay / 2);
    cx.update(|cx| cx.show_osd(OsdA).unwrap());
    cx.executor().advance_clock(delay / 2 + delay / 4);
    assert!(shown(cx).is_some());
    cx.executor().advance_clock(delay);
    assert!(shown(cx).is_none());
    assert_eq!(windows(cx), before);
  }

  #[gpui_kit::test]
  fn another_display_opens_a_new_window(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let before = windows(cx);
    cx.update(|cx| cx.show_osd(OsdA).unwrap());
    let (_, first, _) = shown(cx).unwrap();
    cx.update(|cx| {
      cx.global_mut::<OsdState>().shown.as_mut().unwrap().display = Some(DisplayId::new(99));
      cx.show_osd(OsdA).unwrap();
    });
    let (_, second, _) = shown(cx).unwrap();
    assert!(first != second);
    cx.run_until_parked();
    assert_eq!(windows(cx), before + 1);
  }

  #[gpui_kit::test]
  fn hide_osd_only_hides_its_own(cx: &mut TestAppContext) {
    test_support::setup(cx);
    let before = windows(cx);
    cx.update(|cx| {
      OsdState::hide(cx);
      cx.hide_osd::<OsdA>();
      cx.show_osd(OsdA).unwrap();
      cx.hide_osd::<OsdB>();
    });
    assert_eq!(shown(cx).unwrap().0, OsdA::NAME);
    cx.update(|cx| cx.hide_osd::<OsdA>());
    assert!(shown(cx).is_none());
    cx.run_until_parked();
    assert_eq!(windows(cx), before);
  }

  #[gpui_kit::test]
  fn opens_on_every_edge(cx: &mut TestAppContext) {
    test_support::setup(cx);
    for position in [
      OsdPosition::TopCenter,
      OsdPosition::BottomCenter,
      OsdPosition::CenterLeft,
      OsdPosition::CenterRight,
    ] {
      cx.update(|cx| {
        cx.global_mut::<corona_config::Config>().osd.position = position;
        OsdState::hide(cx);
        cx.show_osd(OsdA).unwrap();
      });
      draw_all(cx);
    }
  }
}
