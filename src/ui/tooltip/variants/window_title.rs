use gpui_kit::{
  App, Context, IntoElement, ParentElement, Pixels, Render, SharedString, Size, Styled, TextStyle,
  Window, component::ActiveTheme, div, px, relative,
};

use crate::ui::tooltip::variants::Tooltip;

const FONT_SIZE: f32 = 12.;
const PADDING: f32 = 4.;
const MAX_WIDTH: f32 = 300.;
const LINE_HEIGHT: f32 = 1.4;
const SLACK: f32 = 1.;

pub struct WindowTitle {
  title: SharedString,
}

impl WindowTitle {
  pub fn new(title: impl Into<SharedString>) -> Self {
    Self {
      title: title.into(),
    }
  }

  fn text_size(&self, window: &Window, cx: &App) -> Size<Pixels> {
    let style = TextStyle {
      font_family: cx.theme().font_family.clone(),
      ..Default::default()
    };

    window
      .text_system()
      .shape_text(
        self.title.clone(),
        px(FONT_SIZE),
        &[style.to_run(self.title.len())],
        Some(px(MAX_WIDTH)),
        None,
      )
      .map(|lines| {
        lines.iter().fold(Size::<Pixels>::default(), |size, line| {
          let line = line.size(px(FONT_SIZE * LINE_HEIGHT));

          Size::new(size.width.max(line.width), size.height + line.height)
        })
      })
      .map(|size| Size::new(size.width + px(SLACK), size.height))
      .unwrap_or_default()
  }
}

impl Tooltip for WindowTitle {
  const NAME: &'static str = "window_title";

  fn size(&self, window: &Window, cx: &App) -> Size<Pixels> {
    let text = self.text_size(window, cx);

    Size::new(
      text.width + px(PADDING * 2.),
      text.height + px(PADDING * 2.),
    )
  }
}

impl Render for WindowTitle {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .size_full()
      .p(px(PADDING))
      .flex()
      .items_center()
      .justify_center()
      .text_center()
      .text_size(px(FONT_SIZE))
      .line_height(relative(LINE_HEIGHT))
      .text_color(theme.tokens.secondary_foreground)
      .child(div().w_full().child(self.title.clone()))
  }
}
