use std::{
  cell::Cell,
  rc::Rc,
  time::{Duration, Instant},
};

use anyhow::Result;
use corona_components::animation::{animation_duration, smooth_retarget::SmoothRetarget};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider, NotificationConfig, NotificationPosition};
use corona_notifications::{NotificationsExt, Urgency};
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

use crate::control_center::NotificationsPanel;

const NAMESPACE: &str = "corona_notification";
const STACK_OFFSET: f32 = 12.;
const SLIDE_SPEED: Duration = Duration::from_millis(250);

type Item = corona_notifications::Notification;

struct Popups {
  items: Vec<Popup>,
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
    let timeout = Duration::from_millis(match item.urgency {
      Urgency::Low | Urgency::Normal => config.timeout_ms,
      Urgency::Critical => config.critical_timeout_ms,
    });
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

pub fn init(cx: &mut App) {
  cx.set_global(NotificationPopups::default());

  let notifications = cx.notifications().notifications.clone();
  let mut old: Vec<u32> = notifications.read(cx).iter().map(|n| n.id).collect();

  cx.observe(&notifications, move |e, cx| {
    let new = e.read(cx).clone();
    NotificationPopups::sync(&new, cx);
    // suppressed ones count as seen, so turning do not disturb off replays nothing
    let dnd = cx.notifications().do_not_disturb(cx);
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
          if popup.open && !popup.hovered {
            popup.remaining = popup.remaining.saturating_sub(now - popup.last_tick);
            if popup.remaining.is_zero() {
              expired.push(popup.item.id);
            }
          }
          popup.last_tick = now;
          let counting = popup.open && !popup.hovered;

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
              NotificationsPanel::notification_card(theme, &popup.item)
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
