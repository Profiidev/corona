use corona_config::{ConfigProvider, placement::Placement};
use corona_surface::{bar::BarState, popup::popup_options};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, Div, Focusable, Global, InteractiveElement,
  KeyDownEvent, ParentElement, Pixels, Render, SharedString, Size, Stateful, Styled, Window,
  component::{ActiveTheme, Root},
  div, px,
};

pub const WIDTH: f32 = 240.;
pub const ROW: f32 = 28.;
pub const SEPARATOR: f32 = 9.;
const PADDING: f32 = 4.;
const BORDER: f32 = 1.;
const GAP: f32 = 4.;

#[derive(Default)]
struct OpenPopup(Option<AnyWindowHandle>);

impl Global for OpenPopup {}

fn close_open(cx: &mut App) {
  let handle = cx.default_global::<OpenPopup>().0.take();
  if let Some(handle) = handle {
    let _ = handle.update(cx, |_, window, _| window.remove_window());
  }
}

pub fn size(content: f32) -> Size<Pixels> {
  Size::new(px(WIDTH), px(content.max(ROW) + PADDING * 2. + BORDER * 2.))
}

pub fn open<V: Render + Focusable>(
  anchor: Bounds<Pixels>,
  size: Size<Pixels>,
  window: &mut Window,
  cx: &mut App,
  build: impl FnOnce(&mut Window, &mut Context<V>) -> V + 'static,
) {
  let placement = BarState::placement(window, cx);
  open_at(anchor, size, placement, window, cx, build);
}

pub fn open_at<V: Render + Focusable>(
  anchor: Bounds<Pixels>,
  size: Size<Pixels>,
  placement: Placement,
  window: &mut Window,
  cx: &mut App,
  build: impl FnOnce(&mut Window, &mut Context<V>) -> V + 'static,
) {
  close_open(cx);
  let options = popup_options(
    window.window_handle(),
    anchor,
    placement,
    size,
    px(GAP),
    true,
  );
  let opened = cx.open_window(options, |window, cx| {
    let view = cx.new(|cx| build(window, cx));
    let focus = view.read(cx).focus_handle(cx);
    window.focus(&focus, cx);
    cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
  });
  if let Ok(handle) = opened.log_err() {
    cx.set_global(OpenPopup(Some(handle.into())));
  }
}

pub fn frame<V: Focusable>(view: &V, cx: &mut Context<V>) -> Div {
  let theme = cx.theme();
  div()
    .track_focus(&view.focus_handle(cx))
    .on_key_down(|e: &KeyDownEvent, window, _| {
      if e.keystroke.key == "escape" {
        window.remove_window();
      }
    })
    .size_full()
    .flex()
    .flex_col()
    .p(px(PADDING))
    .bg(theme.tokens.background)
    .rounded(theme.radius)
    .border(px(BORDER))
    .border_color(
      cx.config()
        .theme
        .popup_border_color(theme.tokens.button_hover.color),
    )
    .text_color(theme.foreground)
}

pub fn row(id: impl Into<SharedString>, cx: &App) -> Stateful<Div> {
  div()
    .id(id.into())
    .flex()
    .items_center()
    .gap_2()
    .h(px(ROW))
    .px_2()
    .rounded(cx.theme().radius)
    .text_sm()
}

pub fn separator(cx: &App) -> Div {
  div()
    .h(px(SEPARATOR))
    .flex()
    .items_center()
    .child(div().h(px(1.)).w_full().bg(cx.theme().border))
}
