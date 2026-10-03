use corona_pipewire::PipewireExt;
use corona_surface::bar::Widget;
use gpui_kit::{Context, IntoElement, Render, Subscription, Window};
use uuid::Uuid;

use crate::{
  control_center::{
    AudioPanel, Standalone,
    audio::utils::{to_slider, volume_icon},
  },
  widgets::button::Button,
};

pub struct AudioButton {
  _subscription: Subscription,
}

impl Widget for AudioButton {
  const NAME: &'static str = "audio";

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let sink = cx.pipewire().default_sink.clone();
    Self {
      _subscription: cx.observe(&sink, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for AudioButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let sink = cx.pipewire().default_sink(cx);
    let volume = to_slider(sink.and_then(|s| s.volumes.first()).copied().unwrap_or(0.));
    let muted = sink.is_some_and(|s| s.mute);

    Button::<_, Standalone<AudioPanel>>::new(cx, "audio-button", volume_icon(volume, muted))
      .danger(muted)
  }
}
