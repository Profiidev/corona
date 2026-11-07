use corona_pipewire::{AudioNode, PipewireExt};
use corona_surface::control_center::ControlCenterPage;
use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  base::StyledExt,
  component::{ActiveTheme, scroll::ScrollableElement},
  div,
};

use crate::panel::audio::{
  state::{DefaultState, StreamState, on_select},
  ui::{audio_node, audio_stream},
};
use rust_i18n::t;

pub(crate) mod icons;
mod listener;
mod state;
mod ui;
mod utils;

pub struct AudioPanel {
  source: DefaultState,
  sink: DefaultState,
  streams: Vec<StreamState>,
  sources: Vec<AudioNode>,
  sinks: Vec<AudioNode>,
  _pipewire_subscriptions: [Subscription; 6],
}

impl ControlCenterPage for AudioPanel {
  const ID: &'static str = "audio";

  fn icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Volume2
  }

  fn title() -> gpui_kit::SharedString {
    rust_i18n::t!("app.control_center.audio").into()
  }

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let pipewire = cx.pipewire();
    let sources = pipewire.list_sources(cx).to_vec();
    let sinks = pipewire.list_sinks(cx).to_vec();
    let streams = pipewire.list_streams(cx).to_vec();

    let source = pipewire.default_source(cx).cloned();
    let sink = pipewire.default_sink(cx).cloned();

    let pipewire_subscriptions = listener::listeners(cx, window);

    let source = DefaultState::create(source, &sources, window, cx);
    let sink = DefaultState::create(sink, &sinks, window, cx);

    let streams = streams
      .into_iter()
      .map(|s| {
        let on_select = on_select(s.id);
        StreamState::create(s, &sinks, window, cx, on_select)
      })
      .collect::<Vec<_>>();

    Self {
      source,
      sink,
      streams,
      sources,
      sinks,
      _pipewire_subscriptions: pipewire_subscriptions,
    }
  }
}

impl Render for AudioPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(audio_node(t!("app.audio.input"), theme, &self.source, true))
      .child(audio_node(t!("app.audio.output"), theme, &self.sink, false))
      .child(
        div()
          .w_full()
          .flex()
          .flex_col()
          .flex_grow_1()
          .min_h_0()
          .bg(theme.tokens.accent)
          .border_color(theme.border)
          .border_1()
          .rounded_xl()
          .p_2()
          .gap_2()
          .child(
            div()
              .child(t!("app.audio.applications"))
              .text_sm()
              .font_bold(),
          )
          .child(
            div().w_full().flex_grow_1().min_h_0().child(
              div()
                .flex()
                .flex_col()
                .gap_1()
                .overflow_y_scrollbar()
                .children(
                  self
                    .streams
                    .iter()
                    .map(|node| audio_stream(theme, node, false)),
                ),
            ),
          ),
      )
  }
}
