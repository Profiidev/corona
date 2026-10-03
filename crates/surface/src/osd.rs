use std::time::Duration;

use anyhow::Result;
use corona_compositor::CompositorExt;
use corona_config::APP_NAME;
use gpui_kit::{
  AnyView, AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Global, IntoElement,
  ParentElement, Pixels, Point, Render, Size, Styled, Task, WeakEntity, Window,
  WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  component::{ActiveTheme, Root},
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  px,
};

use crate::bar::BarExt;

const NAMESPACE: &str = "corona_osd";
const GAP: f32 = 40.;
const BORDER: f32 = 2.;

pub trait Osd: Render {
  const NAME: &'static str;

  fn size(&self, cx: &App) -> Size<Pixels>;

  fn timeout(&self) -> Duration {
    Duration::from_millis(1500)
  }
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
    let display = cx.bar().display_id_for(&monitor);
    let content = osd.size(cx);
    let size = Size::new(
      content.width + px(BORDER * 2.),
      content.height + px(BORDER * 2.),
    );
    let timeout = osd.timeout();
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
    let handle = cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::BOTTOM,
          exclusive_zone: None,
          exclusive_edge: None,
          margin: Some((px(0.), px(0.), px(GAP), px(0.))),
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

struct BaseOsd {
  content: AnyView,
}

impl Render for BaseOsd {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .size_full()
      .bg(theme.tokens.background)
      .rounded(theme.radius * 2)
      .border(px(BORDER))
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
