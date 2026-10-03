use std::{iter, time::Duration};

use corona_mpris::{MprisExt, PlaybackStatus, Player};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
  Styled, Subscription, Task, Window,
  base::{
    FocusableExt, IndexPath,
    slider::{SliderEvent, SliderState},
  },
  component::{
    ActiveTheme,
    select::{Select, SelectEvent, SelectItem, SelectState},
  },
  div, px,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};

pub(crate) mod player;

/// how often the position moves on while playing
const TICK: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
struct PlayerItem {
  /// None: follow whichever player is active
  name: Option<String>,
  title: SharedString,
}

impl SelectItem for PlayerItem {
  type Value = Option<String>;

  fn title(&self) -> SharedString {
    self.title.clone()
  }

  fn value(&self) -> &Self::Value {
    &self.name
  }
}

fn player_items(players: &[Player]) -> Vec<PlayerItem> {
  let active = PlayerItem {
    name: None,
    title: "Active player".into(),
  };
  iter::once(active)
    .chain(players.iter().map(|p| PlayerItem {
      name: Some(p.name.clone()),
      title: p.identity.clone().into(),
    }))
    .collect()
}

pub struct MediaPanel {
  /// the player picked in the header, None follows the active one
  pinned: Option<String>,
  select: Entity<SelectState<Vec<PlayerItem>>>,
  progress: Entity<SliderState>,
  /// the fraction the progress thumb is dragged to, the ticking position leaves it alone meanwhile
  seeking: Option<f32>,
  _subscriptions: Vec<Subscription>,
  _ticker: Task<()>,
}

impl ControlCenterPanel for MediaPanel {
  const TYPE: ControlCenterType = ControlCenterType::Media;
  const HEIGHT: f32 = 270.0;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let mpris = cx.mpris().clone();
    let items = player_items(mpris.list_players(cx));
    let select = cx.new(|cx| SelectState::new(items, Some(IndexPath::new(0)), window, cx));
    let progress = cx.new(|_| SliderState::new().min(0.).max(1.).step(0.001));

    let subscriptions = vec![
      cx.observe_in(&mpris.players, window, |this, players, window, cx| {
        this.update_players(&players.read(cx).clone(), window, cx);
      }),
      cx.observe_in(&mpris.active, window, |this, _, window, cx| {
        this.sync_progress(window, cx);
        cx.notify();
      }),
      cx.subscribe_in(
        &select,
        window,
        |this, _, event: &SelectEvent<Vec<PlayerItem>>, window, cx| {
          let SelectEvent::Confirm(Some(name)) = event else {
            return;
          };
          this.pinned = name.clone();
          this.sync_progress(window, cx);
          cx.notify();
        },
      ),
      cx.subscribe(&progress, |this, _, event: &SliderEvent, cx| match event {
        SliderEvent::Change(value) => {
          this.seeking = Some(value.start());
          cx.notify();
        }
        SliderEvent::Release(value) => {
          this.seeking = None;
          this.seek(value.start(), cx);
        }
      }),
    ];

    // players only report the position when it jumps, it moves on in between
    let ticker = cx.spawn_in(window, async move |this, cx| {
      loop {
        cx.background_executor().timer(TICK).await;
        let tick = this.update_in(cx, |this, window, cx| {
          if this
            .shown(cx)
            .is_some_and(|p| p.status == PlaybackStatus::Playing)
          {
            this.sync_progress(window, cx);
            cx.notify();
          }
        });
        if tick.is_err() {
          break;
        }
      }
    });

    let mut panel = Self {
      pinned: None,
      select,
      progress,
      seeking: None,
      _subscriptions: subscriptions,
      _ticker: ticker,
    };
    panel.sync_progress(window, cx);
    panel
  }

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![
      Select::new(&self.select)
        .w(px(160.))
        .ml_auto()
        .focus_ring(false)
        .cursor_pointer()
        .into_any_element(),
    ]
  }
}

impl MediaPanel {
  /// the pinned player, or the active one when nothing is pinned or the pinned player quit
  fn shown<'c>(&self, cx: &'c App) -> Option<&'c Player> {
    let mpris = cx.mpris();
    self
      .pinned
      .as_deref()
      .and_then(|name| mpris.player(name, cx))
      .or_else(|| mpris.active_player(cx))
  }

  fn update_players(&mut self, players: &[Player], window: &mut Window, cx: &mut Context<Self>) {
    let items = player_items(players);
    // a pinned player that quit falls back to "Active player", the first item
    let index = items
      .iter()
      .position(|item| item.name.is_some() && item.name == self.pinned)
      .unwrap_or(0);
    self.select.update(cx, |state, cx| {
      state.set_items(items, window, cx);
      state.set_selected_index(Some(IndexPath::new(index)), window, cx);
    });
    self.sync_progress(window, cx);
    cx.notify();
  }

  fn sync_progress(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.seeking.is_some() {
      return;
    }
    let fraction = self.shown(cx).and_then(|player| {
      let length = player.length.filter(|l| !l.is_zero())?;
      Some(player.position().as_secs_f32() / length.as_secs_f32())
    });
    if let Some(fraction) = fraction {
      self
        .progress
        .update(cx, |state, cx| state.set_value(fraction, window, cx));
    }
  }

  fn seek(&mut self, fraction: f32, cx: &mut Context<Self>) {
    let Some((name, length)) = self
      .shown(cx)
      .and_then(|p| Some((p.name.clone(), p.length?)))
    else {
      return;
    };
    let seek = cx
      .mpris()
      .set_position(&name, length.mul_f32(fraction.clamp(0., 1.)), cx);
    cx.spawn(async move |_, _| {
      seek.await.log_err().ok();
    })
    .detach();
  }
}

impl Render for MediaPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.player(theme, cx))
  }
}
