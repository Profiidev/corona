use std::{
  cell::Cell,
  io::Cursor,
  rc::Rc,
  sync::{OnceLock, mpsc},
  thread::spawn,
  time::{Duration, Instant},
};

use anyhow::Result;
use corona_components::animation::{animation_duration, smooth_retarget::SmoothRetarget};
use corona_compositor::CompositorExt;
use corona_config::{
  APP_NAME, ConfigProvider, NotificationConfig, NotificationPosition, observe_section,
};
use corona_notifications::{Filter, NotificationsExt, Urgency, strip_markup};
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Entity, Global, IntoElement,
  ParentElement, Pixels, Point, Render, Size, StatefulInteractiveElement, Styled, Window,
  WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  base::{ElementExt, Root},
  component::ActiveTheme,
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  px, relative,
};
use rodio::{Decoder, DeviceSinkBuilder, Source};

use crate::{
  control_center::{NotificationsPanel, ReplyInputs},
  widgets::filter_regex,
};

const NAMESPACE: &str = "corona_notification";
const STACK_OFFSET: f32 = 12.;
const SLIDE_SPEED: Duration = Duration::from_millis(250);
/// the timeout of a popup that stays until clicked
const NEVER: Duration = Duration::MAX;

type Item = corona_notifications::Notification;

struct Popups {
  items: Vec<Popup>,
  replies: ReplyInputs,
}

struct Popup {
  item: Item,
  // false on slide out
  open: bool,
  anim: SmoothRetarget,
  slot: SmoothRetarget,
  height: Rc<Cell<Pixels>>,
  timeout: Duration,
  remaining: Duration,
  last_tick: Instant,
  hovered: bool,
}

impl Popup {
  fn new(item: Item, slot: usize, config: &NotificationConfig) -> Self {
    // the app's own timeout, or the configured one by urgency
    let timeout = match (item.expire_timeout, item.urgency) {
      (0, _) => NEVER,
      (ms @ 1.., _) => Duration::from_millis(ms as u64),
      (_, Urgency::Low | Urgency::Normal) => Duration::from_millis(config.timeout_ms),
      (_, Urgency::Critical) => Duration::from_millis(config.critical_timeout_ms),
    };
    Self {
      item,
      open: true,
      timeout,
      remaining: timeout,
      last_tick: Instant::now(),
      hovered: false,
      anim: SmoothRetarget::new(0.),
      slot: SmoothRetarget::new(-(slot as f32)),
      height: Rc::default(),
    }
  }
}

#[derive(Default)]
struct NotificationPopups {
  windows: Vec<PopupWindow>,
}

struct PopupWindow {
  handle: AnyWindowHandle,
  display: Option<DisplayId>,
  view: Entity<Popups>,
}

impl Global for NotificationPopups {}

/// `[notification] filter_regex` as a [`Filter`]
fn filter(config: &NotificationConfig) -> Filter {
  let re = filter_regex(&config.filter_regex);
  Filter(Box::new(move |n| {
    re.as_ref().is_some_and(|re| {
      [&n.app_name, &n.summary, &strip_markup(&n.body)]
        .iter()
        .any(|text| re.is_match(text))
    })
  }))
}

pub fn init(cx: &mut App) {
  cx.set_global(NotificationPopups::default());
  cx.set_global(filter(&cx.config().notification));
  observe_section(
    cx,
    |c| &c.notification,
    |config, cx| cx.set_global(filter(config)),
  );

  let notifications = cx.notifications().notifications.clone();
  let mut old: Vec<u32> = notifications.read(cx).iter().map(|n| n.id).collect();

  cx.observe(&notifications, move |e, cx| {
    let new = e.read(cx).clone();
    NotificationPopups::sync(&new, cx);
    // suppressed ones count as seen, so turning do not disturb off replays nothing
    let dnd = cx.notifications().do_not_disturb(cx);
    if !dnd && new.iter().any(|n| !old.contains(&n.id)) {
      play_sound();
    }
    for notification in new.iter().filter(|n| !dnd && !old.contains(&n.id)) {
      if let Err(e) = NotificationPopups::show(notification.clone(), cx) {
        tracing::error!("failed to show notification popup: {e:?}");
      }
    }
    old = new.iter().map(|n| n.id).collect();
  })
  .detach();

  let dnd = cx.notifications().do_not_disturb.clone();
  cx.observe(&dnd, |dnd, cx| {
    if !*dnd.read(cx) {
      return;
    }
    let ids: Vec<u32> = NotificationPopups::views(cx)
      .iter()
      .flat_map(|v| v.read(cx).items.iter().map(|p| p.item.id))
      .collect();
    for id in ids {
      NotificationPopups::hide(id, cx);
    }
  })
  .detach();
}

/// one thread owns the sink and the decoded sound, set up on first sound and
/// kept for the rest
fn play_sound() {
  static SOUND: &[u8] = include_bytes!("../../../../assets/sounds/notification.oga");
  static PLAY: OnceLock<mpsc::Sender<()>> = OnceLock::new();
  let tx = PLAY.get_or_init(|| {
    let (tx, rx) = mpsc::channel();
    spawn(move || {
      let mut player = None;
      for _ in rx {
        let result = match &player {
          Some(player) => Ok(player),
          // a failed setup is retried on the next sound
          None => (|| {
            let mut sink = DeviceSinkBuilder::open_default_sink()?;
            sink.log_on_drop(false);
            anyhow::Ok((sink, Decoder::new(Cursor::new(SOUND))?.buffered()))
          })()
          .map(|p| &*player.insert(p)),
        };
        match result {
          Ok((sink, sound)) => sink.mixer().add(sound.clone()),
          Err(e) => tracing::error!("failed to play notification sound: {e:?}"),
        }
      }
    });
    tx
  });
  let _ = tx.send(());
}

impl NotificationPopups {
  fn views(cx: &App) -> Vec<Entity<Popups>> {
    cx.global::<Self>()
      .windows
      .iter()
      .map(|w| w.view.clone())
      .collect()
  }

  fn sync(new: &[Item], cx: &mut App) {
    let mut gone = Vec::new();
    for view in Self::views(cx) {
      view.update(cx, |this, cx| {
        for popup in &mut this.items {
          match new.iter().find(|n| n.id == popup.item.id) {
            Some(n) => popup.item = n.clone(),
            None => gone.push(popup.item.id),
          }
        }
        cx.notify();
      });
    }
    for id in gone {
      Self::hide(id, cx);
    }
  }

  fn show(notification: Item, cx: &mut App) -> Result<()> {
    let monitor = cx.compositor().active_monitor(cx).name.clone();
    let display = display_id_for(&monitor, cx);
    let config = cx.config().notification.clone();

    let existing = cx
      .global::<Self>()
      .windows
      .iter()
      .find(|w| w.display == display)
      .map(|w| w.view.clone());
    match existing {
      Some(view) => view.update(cx, |this, cx| {
        let slot = this.items.iter().filter(|p| p.open).count();
        this.items.push(Popup::new(notification, slot, &config));
        cx.notify();
      }),
      None => {
        let view = cx.new(|_| Popups {
          items: vec![Popup::new(notification, 0, &config)],
          replies: ReplyInputs::default(),
        });
        Self::open(view, display, cx)?;
      }
    }
    Ok(())
  }

  fn hide(id: u32, cx: &mut App) {
    let Some(view) = Self::views(cx)
      .into_iter()
      .find(|v| v.read(cx).items.iter().any(|p| p.item.id == id))
    else {
      return;
    };
    let mut started = false;
    view.update(cx, |this, cx| {
      if let Some(popup) = this.items.iter_mut().find(|p| p.item.id == id && p.open) {
        popup.open = false;
        started = true;
        cx.notify();
      }
    });
    if !started {
      return;
    }

    let speed = animation_duration(SLIDE_SPEED, cx);
    cx.spawn(async move |cx| {
      cx.background_executor().timer(speed).await;
      cx.update(|cx| {
        view.update(cx, |this, cx| {
          this.items.retain(|p| p.item.id != id || p.open);
          cx.notify();
        });
        Self::close_empty(cx);
      });
    })
    .detach();
  }

  fn open(view: Entity<Popups>, display: Option<DisplayId>, cx: &mut App) -> Result<()> {
    let config = cx.config().notification.clone();
    let handle = cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP
            | match config.position {
              NotificationPosition::TopLeft => Anchor::LEFT,
              NotificationPosition::TopRight => Anchor::RIGHT,
            },
          exclusive_zone: None,
          exclusive_edge: None,
          margin: Some((px(config.offset), px(0.), px(0.), px(0.))),
          layer: Layer::Overlay,
          namespace: NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::OnDemand,
        }),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: Point::default(),
          size: Size::new(px(config.width + config.offset), px(1.)),
        })),
        display_id: display,
        ..Default::default()
      },
      {
        let view = view.clone();
        |window, cx| cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
      },
    )?;

    cx.global_mut::<Self>().windows.push(PopupWindow {
      handle: handle.into(),
      display,
      view,
    });
    Ok(())
  }

  fn close_empty(cx: &mut App) {
    let empty: Vec<_> = cx
      .global::<Self>()
      .windows
      .iter()
      .filter(|w| w.view.read(cx).items.is_empty())
      .map(|w| w.handle)
      .collect();
    cx.global_mut::<Self>()
      .windows
      .retain(|w| !empty.contains(&w.handle));
    for handle in empty {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
  }
}

impl Render for Popups {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self
      .replies
      .sync(self.items.iter().map(|p| &p.item), window, cx);
    let speed = animation_duration(SLIDE_SPEED, cx);
    let config = cx.config().notification.clone();
    let (width, gap) = (config.width, config.offset);
    // the window is `gap` wider than a card, the gap on the screen edge's side;
    // a card slides out over that edge
    let (shown_at, out) = match config.position {
      NotificationPosition::TopLeft => (gap, -(width + gap)),
      NotificationPosition::TopRight => (0., width + gap),
    };
    let theme = cx.theme();
    let mut slot = 0;
    let mut tops = Vec::new();
    let mut expired = Vec::new();
    let now = Instant::now();

    let el = div().size_full().child(
      div()
        .absolute()
        .top_0()
        .left_0()
        .w(px(width + gap))
        .children(self.items.iter_mut().map(|popup| {
          popup.anim.retarget(if popup.open { 1. } else { 0. }, speed);
          if popup.open {
            popup.slot.retarget(-(slot as f32), speed / 4);
            slot += 1;
          }
          // typing a reply holds it like hovering
          let held = popup.hovered || self.replies.typing(popup.item.id, window, cx);
          let counting = popup.open && !held && popup.timeout != NEVER;
          if counting {
            popup.remaining = popup.remaining.saturating_sub(now - popup.last_tick);
            if popup.remaining.is_zero() {
              expired.push(popup.item.id);
            }
          }
          popup.last_tick = now;

          let (progress, sliding) = popup.anim.value();
          let (at, moving) = popup.slot.value();
          if sliding || moving || counting {
            window.request_animation_frame();
          }
          let top = px((-STACK_OFFSET * at).round());
          tops.push((top, popup.height.clone()));
          let height = popup.height.clone();
          div()
            .absolute()
            .top(top)
            .left(px((shown_at + out * (1. - progress)).round()))
            .w(px(width))
            .opacity(progress)
            .child({
              let id = popup.item.id;
              let left = popup.remaining.as_secs_f32() / popup.timeout.as_secs_f32();
              NotificationsPanel::notification_card(cx, &popup.item, self.replies.get(id))
                .bg(theme.colors.accent.opacity(config.background_opacity))
                .relative()
                .child(
                  div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .w(relative(left))
                    .overflow_hidden()
                    .child(
                      div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(width))
                        .h_full()
                        .rounded_xl()
                        .border_t_2()
                        .border_color(theme.colors.primary),
                    ),
                )
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                  if let Some(popup) = this.items.iter_mut().find(|p| p.item.id == id) {
                    popup.hovered = *hovered;
                    popup.last_tick = Instant::now();
                    cx.notify();
                  }
                }))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                  cx.stop_propagation();
                  cx.notifications().clone().mark_read(id, cx);
                  NotificationPopups::hide(id, cx);
                })
            })
            .on_prepaint(move |bounds, _, _| height.set(bounds.size.height))
        }))
        .on_prepaint({
          move |_, window, _| {
            let height = tops
              .iter()
              .map(|(top, h)| *top + h.get())
              .fold(px(1.), Pixels::max)
              .ceil();
            if window.viewport_size().height != height {
              window.resize(Size::new(px(width + gap), height));
            }
          }
        }),
    );

    if !expired.is_empty() {
      cx.defer(move |cx| {
        for id in expired {
          NotificationPopups::hide(id, cx);
        }
      });
    }
    el
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::SystemTime;

  fn item(app_name: &str, summary: &str, body: &str, urgency: Urgency) -> Item {
    Item {
      id: 1,
      app_name: app_name.into(),
      app_icon: String::new(),
      image: None,
      summary: summary.into(),
      body: body.into(),
      actions: Vec::new(),
      urgency,
      desktop_entry: None,
      reply_placeholder: None,
      expire_timeout: -1,
      resident: false,
      time: SystemTime::UNIX_EPOCH,
      read: false,
    }
  }

  fn drops(pattern: &str, n: &Item) -> bool {
    let config = NotificationConfig {
      filter_regex: pattern.into(),
      ..Default::default()
    };
    (filter(&config).0)(n)
  }

  #[test]
  fn empty_or_broken_regex_keeps_everything() {
    let n = item("Spotify", "Now playing", "Song", Urgency::Normal);
    assert!(!drops("", &n));
    assert!(!drops("(", &n));
  }

  #[test]
  fn regex_matches_any_text_field_case_insensitively() {
    let n = item("Spotify", "Now playing", "Some Song", Urgency::Normal);
    assert!(drops("^spotify$", &n));
    assert!(drops("now PLAYING", &n));
    assert!(drops("song", &n));
    assert!(!drops("discord", &n));
    // the fields are matched one by one, not joined
    assert!(!drops("Spotify Now", &n));
    // the body as its text, without markup
    let n = item("a", "b", "<b>Some</b> &amp; Song", Urgency::Normal);
    assert!(drops("some & song", &n));
    assert!(!drops("<b>", &n));
  }

  #[test]
  fn popup_timeout_by_urgency() {
    let config = NotificationConfig {
      timeout_ms: 1000,
      critical_timeout_ms: 9000,
      ..Default::default()
    };
    for (urgency, ms) in [
      (Urgency::Low, 1000),
      (Urgency::Normal, 1000),
      (Urgency::Critical, 9000),
    ] {
      let popup = Popup::new(item("a", "b", "c", urgency), 0, &config);
      assert_eq!(popup.timeout, Duration::from_millis(ms), "{urgency:?}");
      assert_eq!(popup.remaining, popup.timeout);
      assert!(popup.open && !popup.hovered);
    }
  }

  #[test]
  fn popup_timeout_from_the_app() {
    let config = NotificationConfig {
      timeout_ms: 1000,
      critical_timeout_ms: 9000,
      ..Default::default()
    };
    for (urgency, expire_timeout, timeout) in [
      (Urgency::Normal, -1, Duration::from_millis(1000)),
      (Urgency::Critical, -5, Duration::from_millis(9000)),
      (Urgency::Normal, 0, NEVER),
      (Urgency::Critical, 0, NEVER),
      (Urgency::Low, 250, Duration::from_millis(250)),
      (Urgency::Critical, 250, Duration::from_millis(250)),
    ] {
      let item = Item {
        expire_timeout,
        ..item("a", "b", "c", urgency)
      };
      let popup = Popup::new(item, 0, &config);
      assert_eq!(popup.timeout, timeout, "{urgency:?} {expire_timeout}");
      assert_eq!(popup.remaining, timeout);
    }
  }
}
