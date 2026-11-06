use crate::{MprisExt, PlaybackStatus};
use corona_components::components::card::CardExt;
use corona_components::components::list::muted;
use gpui_kit::{
  App, Context, Div, Empty, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render,
  SharedString, Stateful, StatefulInteractiveElement, Styled, StyledImage, Subscription, Window,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
  },
  div, img, px,
};

use corona_surface::{
  control_center::{ControlCenter, ControlCenterPage},
  dashboard::DashboardCard,
};
use corona_utils::error::spawn_logged;
use rust_i18n::t;

use crate::ui::page::MediaPanel;

const ART: f32 = 64.;

fn card(id: &'static str, cx: &App) -> Stateful<Div> {
  div().id(id).flex().p_2().gap_2().card(cx)
}

fn media(cx: &App) -> Option<Stateful<Div>> {
  let theme = cx.theme();
  let player = cx.mpris().active_player(cx)?;
  let name = player.name.clone();
  let playing = player.status == PlaybackStatus::Playing;
  let art = player.art_source();
  let control = |id: &'static str, icon: IconName, enabled: bool, name: &str| {
    let name = name.to_string();
    Button::new(id)
      .icon(icon)
      .ghost()
      .small()
      .cursor_pointer()
      .disabled(!enabled)
      .on_click(move |_, _, cx| {
        cx.stop_propagation();
        let mpris = cx.mpris();
        match icon {
          IconName::SkipBack => spawn_logged(cx, mpris.previous(&name)),
          IconName::SkipForward => spawn_logged(cx, mpris.next(&name)),
          _ => spawn_logged(cx, mpris.play_pause(&name)),
        }
      })
  };

  Some(
    card("dashboard-media", cx)
      .flex_1()
      .min_w_0()
      .items_center()
      .cursor_pointer()
      .on_click(|_, window, cx| ControlCenter::navigate_id(MediaPanel::ID, window, cx))
      .child(
        div()
          .size(px(ART))
          .flex_none()
          .rounded_lg()
          .overflow_hidden()
          .bg(theme.colors.background)
          .flex()
          .items_center()
          .justify_center()
          .child(match art {
            Some(source) => img(source)
              .size_full()
              .rounded_lg()
              .object_fit(ObjectFit::Cover)
              .into_any_element(),
            None => Icon::new(IconName::Music).into_any_element(),
          }),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .min_w_0()
          .flex_1()
          .child(
            div().text_sm().font_semibold().truncate().child(
              player
                .title
                .clone()
                .unwrap_or_else(|| player.identity.clone()),
            ),
          )
          .child(muted(player.artists.join(", "), cx))
          .child(
            div()
              .flex()
              .child(control(
                "dashboard-previous",
                IconName::SkipBack,
                player.can_go_previous,
                &name,
              ))
              .child(control(
                "dashboard-play",
                if playing {
                  IconName::Pause
                } else {
                  IconName::Play
                },
                player.can_play || player.can_pause,
                &name,
              ))
              .child(control(
                "dashboard-next",
                IconName::SkipForward,
                player.can_go_next,
                &name,
              )),
          ),
      ),
  )
}

/// The active player, nothing without one
pub struct MediaCard {
  _subscriptions: [Subscription; 2],
}

impl DashboardCard for MediaCard {
  const ID: &'static str = "media";

  fn title() -> SharedString {
    t!("app.control_center.media").into()
  }

  fn init(_: &mut Window, cx: &mut Context<Self>) -> Self {
    let players = cx.mpris().players.clone();
    let active = cx.mpris().active.clone();
    Self {
      _subscriptions: [
        cx.observe(&players, |_, _, cx| cx.notify()),
        cx.observe(&active, |_, _, cx| cx.notify()),
      ],
    }
  }
}

impl Render for MediaCard {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // `Empty` takes no layout, so the row's gap leaves it out
    media(cx).map_or_else(|| Empty.into_any_element(), |d| d.into_any_element())
  }
}
