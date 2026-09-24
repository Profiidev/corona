use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  base::StyledExt,
  component::{ActiveTheme, scroll::ScrollableElement},
  div,
};

use crate::{
  error::ErrorLogExt,
  integration::pipewire::{AudioNode, PipewireAudio, PipewireExt},
  ui::app::control_center::{
    ControlCenterPanel,
    audio::{
      state::{DEFAULT_SINK_ID, DefaultState, StreamState},
      ui::{audio_node, audio_stream},
    },
  },
};

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

impl ControlCenterPanel for AudioPanel {
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
        let stream = s.id;
        let on_select = move |id, audio: PipewireAudio<'_>| {
          if id == DEFAULT_SINK_ID {
            audio.reset_target(stream).log_err().ok();
            return;
          }
          audio.set_target(stream, id).log_err().ok();
        };

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
      .child(audio_node("Input", theme, &self.source, true))
      .child(audio_node("Output", theme, &self.sink, false))
      .child(
        div()
          .w_full()
          .flex()
          .flex_col()
          .flex_grow_1()
          .min_h_0()
          .bg(theme.tokens.accent)
          .rounded_xl()
          .p_2()
          .gap_2()
          .child(div().child("Applications").text_sm().font_bold())
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
