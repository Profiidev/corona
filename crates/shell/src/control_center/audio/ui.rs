use corona_components::components::window_icon::WindowIcon;
use corona_desktop::entry::name_for_names;
use corona_pipewire::{AudioNode, PipewireExt};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Div, IntoElement, ParentElement, SharedString, Styled,
  assets::IconName,
  base::{Disableable, FocusableExt, StyledExt},
  component::{
    Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    select::Select,
    slider::Slider,
  },
  div,
};

use crate::control_center::audio::state::{DefaultState, NodeState, StreamState};

fn audio_btns(state: &NodeState, node: Option<&AudioNode>, mic: bool) -> Div {
  div()
    .flex_grow_1()
    .flex()
    .items_center()
    .gap_2()
    .child(
      Select::new(&state.select)
        .focus_ring(false)
        .cursor_pointer()
        .disabled(node.is_none()),
    )
    .child(
      Button::new(format!(
        "mute_{}",
        node.map_or(if mic { -1 } else { -2 }, |n| i64::from(n.id))
      ))
      .disabled(node.is_none())
      .cursor_pointer()
      .icon(if mic {
        if state.muted {
          IconName::MicOff
        } else {
          IconName::Mic
        }
      } else {
        if state.muted {
          IconName::VolumeOff
        } else if state.volume < f32::EPSILON {
          IconName::VolumeX
        } else if state.volume < 0.5 {
          IconName::Volume1
        } else {
          IconName::Volume2
        }
      })
      .with_variant(if state.muted {
        ButtonVariant::Danger
      } else {
        ButtonVariant::Default
      })
      .on_click({
        let id = node.map(|n| n.id).unwrap_or(0);
        let muted = state.muted;
        move |_, _, cx| {
          let audio = cx.pipewire().audio();
          audio.set_mute(id, !muted).log_err().ok();
        }
      }),
    )
}

fn audio_slider(state: &NodeState, node: Option<&AudioNode>, theme: &Theme) -> Div {
  div()
    .flex()
    .items_center()
    .gap_1()
    .child(
      Slider::new(&state.slider)
        .px_2()
        .cursor_pointer()
        .disabled(node.is_none()),
    )
    .child(
      div()
        .text_sm()
        .text_color(theme.tokens.muted_foreground)
        .w_10()
        .flex()
        .justify_end()
        .child(SharedString::from(format!("{:.0}%", state.volume * 100.))),
    )
}

fn audio_controls(theme: &Theme, node: &DefaultState, mic: bool) -> Div {
  div()
    .w_full()
    .flex()
    .flex_col()
    .gap_1()
    .child(audio_btns(&node.state, node.node.as_ref(), mic))
    .child(audio_slider(&node.state, node.node.as_ref(), theme))
}

pub fn audio_node(title: impl IntoElement, theme: &Theme, node: &DefaultState, mic: bool) -> Div {
  div()
    .w_full()
    .flex()
    .flex_col()
    .gap_2()
    .p_2()
    .bg(theme.tokens.accent)
    .rounded_xl()
    .child(div().child(title).text_sm().font_bold())
    .child(audio_controls(theme, node, mic))
}

pub fn audio_stream(theme: &Theme, node: &StreamState, mic: bool) -> Div {
  let name = node
    .node
    .nickname
    .clone()
    .unwrap_or_else(|| node.node.description.clone());

  let label = node
    .node
    .nickname
    .clone()
    .or_else(|| name_for_names(node.node.app.iter().map(String::as_str)))
    .unwrap_or_else(|| node.node.description.clone());

  div()
    .w_full()
    .flex()
    .p_2()
    .items_center()
    .rounded_xl()
    .bg(theme.tokens.background)
    .child(
      WindowIcon::new(name, ("node-icon", node.node.id))
        .names(node.node.app.clone())
        .max_size_10(),
    )
    .child(
      div()
        .w_full()
        .flex()
        .flex_col()
        .child(
          div()
            .flex()
            .gap_2()
            .w_full()
            .items_center()
            .child(div().child(label.clone()).text_sm().font_bold().ml_2())
            .child(audio_btns(&node.state, Some(&node.node), mic)),
        )
        .child(audio_slider(&node.state, Some(&node.node), theme)),
    )
}
