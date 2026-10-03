use std::time::Duration;

use corona_components::{
  animation::size::SizeAnimation,
  components::scrolling_text::{ScrollingText, ScrollingTextExt, ScrollingTextState},
};
use corona_mpris::{MprisExt, PlaybackStatus};
use corona_surface::{
  bar::{BarStyle, Widget},
  panel::WdigetPanelExt,
};
use corona_utils::{error::ErrorLogExt, ticker::TickerExt};
use gpui_kit::{
  AppContext, Axis, Context, Empty, Entity, InteractiveElement, IntoElement, ObjectFit,
  ParentElement, Render, StatefulInteractiveElement, Styled, StyledImage, Subscription, Task,
  Window,
  assets::IconName,
  component::{ActiveTheme, Icon},
  div, img, px,
};
use uuid::Uuid;

use corona_components::components::progress_ring::ring;

use crate::control_center::{MediaPanel, Standalone};

const TICK: Duration = Duration::from_secs(1);
const RING: f32 = 20.;
const RING_WIDTH: f32 = 2.;
const ART: f32 = 14.;
const WIDTH_CHANGE: Duration = Duration::from_millis(400);

pub struct ActivePlayer {
  scrolling: Entity<ScrollingTextState>,
  size: SizeAnimation,
  _subscriptions: [Subscription; 2],
  _ticker: Task<()>,
}

impl Widget for ActivePlayer {
  const NAME: &'static str = "player";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    let mpris = cx.mpris().clone();
    let subscriptions = [
      cx.observe(&mpris.players, |_, _, cx| cx.notify()),
      cx.observe(&mpris.active, |this, _, cx| {
        if cx.mpris().active_player(cx).is_none() {
          this.size.reset();
          this.scrolling.reset_hover(cx);
        }
        cx.notify();
      }),
    ];

    let ticker = cx.ticker(TICK, |_, cx| {
      if cx
        .mpris()
        .active_player(cx)
        .is_some_and(|p| p.status == PlaybackStatus::Playing)
      {
        cx.notify();
      }
    });

    Self {
      scrolling: cx.new(|_| ScrollingTextState::default()),
      size: SizeAnimation::new(WIDTH_CHANGE),
      _subscriptions: subscriptions,
      _ticker: ticker,
    }
  }
}

impl Render for ActivePlayer {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(player) = cx.mpris().active_player(cx) else {
      return Empty.into_any_element();
    };
    let theme = cx.theme();

    let progress = player.length.filter(|l| !l.is_zero()).map_or(0., |l| {
      (player.position().as_secs_f32() / l.as_secs_f32()).min(1.)
    });
    let title = match (&player.title, player.artists.is_empty()) {
      (Some(title), false) => format!("{title} - {}", player.artists.join(", ")),
      (Some(title), true) => title.clone(),
      (None, _) => player.identity.clone(),
    };

    let placeholder = || {
      Icon::new(IconName::Music)
        .size(px(ART - 4.))
        .into_any_element()
    };
    let art = div()
      .size(px(ART))
      .rounded_full()
      .overflow_hidden()
      .flex()
      .items_center()
      .justify_center()
      .bg(theme.colors.background)
      .child(match player.art_source() {
        Some(source) => img(source)
          .size_full()
          .rounded_full()
          .object_fit(ObjectFit::Cover)
          .with_fallback(placeholder)
          .into_any_element(),
        None => placeholder(),
      });

    div()
      .id("active-player")
      .bar_pill(window, cx)
      .pl(px(2.))
      .cursor_pointer()
      .on_hover(self.scrolling.on_hover())
      .child(
        div()
          .relative()
          .flex_none()
          .size(px(RING))
          .flex()
          .items_center()
          .justify_center()
          .child(art)
          .child(ring(progress, theme.colors.primary, RING_WIDTH)),
      )
      .child({
        let title = ScrollingText::new(self.scrolling.clone()).content(title);
        self.size.animate(
          "active-player-title",
          Axis::Horizontal,
          title.width(window),
          cx,
          title,
        )
      })
      .on_click(cx.listener(|_, _, window, cx| {
        let _ = cx.toggle_panel::<Standalone<MediaPanel>>(window).log_err();
      }))
      .into_any_element()
  }
}
