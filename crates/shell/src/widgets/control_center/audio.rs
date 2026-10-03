use corona_pipewire::{AudioNode, PipewireExt, volume::to_slider};
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Subscription, Window};
use uuid::Uuid;

use crate::{
  control_center::{AudioPanel, Standalone},
  icons::volume_icon,
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
    let volume = to_slider(sink.map_or(0., AudioNode::volume));
    let muted = sink.is_some_and(|s| s.mute);

    Button::<_, Standalone<AudioPanel>>::new(cx, "audio-button", volume_icon(volume, muted))
      .danger(muted)
  }
}
