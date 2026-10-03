use std::{
  cell::Cell,
  rc::Rc,
  time::{Duration, SystemTime},
};

use corona_pipewire::{CaptureKind, PipewireExt};
use corona_surface::bar::{BarStyle, Widget};
use corona_utils::ticker::TickerExt;
use gpui_kit::{
  App, Bounds, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, MouseButton,
  ParentElement, Pixels, Render, SharedString, Styled, Subscription, Task, Window,
  assets::IconName,
  base::ElementExt,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
  px,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
  icons::{capture_icon, capture_label},
  widgets::popup::{self, ROW, SEPARATOR},
};

const ICON_SIZE: f32 = 16.;
const LOG_ROWS: usize = 10;

const KINDS: [CaptureKind; 3] = [
  CaptureKind::Microphone,
  CaptureKind::Camera,
  CaptureKind::Screen,
];

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Options {
  pub hide_when_idle: bool,
}

pub struct Privacy {
  options: Options,
  bounds: Rc<Cell<Bounds<Pixels>>>,
  _subscription: Subscription,
}

impl Widget for Privacy {
  const NAME: &'static str = "privacy";

  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Options) -> Self {
    let captures = cx.pipewire().captures.clone();
    Self {
      options,
      bounds: Rc::default(),
      _subscription: cx.observe(&captures, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for Privacy {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let pipewire = cx.pipewire();
    let recording: Vec<CaptureKind> = KINDS
      .into_iter()
      .filter(|kind| pipewire.is_capturing(*kind, cx))
      .collect();
    if recording.is_empty() && self.options.hide_when_idle {
      return div().into_any_element();
    }

    let theme = cx.theme();
    let (active, idle) = (theme.primary, theme.muted_foreground);
    let bounds = self.bounds.clone();

    div()
      .id("privacy")
      .bar_pill(window, cx)
      .gap_1p5()
      .cursor_pointer()
      .when(recording.is_empty(), |d| {
        d.child(
          Icon::new(IconName::Shield)
            .with_size(px(ICON_SIZE))
            .text_color(idle),
        )
      })
      .children(recording.into_iter().map(|kind| {
        Icon::new(capture_icon(kind))
          .with_size(px(ICON_SIZE))
          .text_color(active)
      }))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|this, _, window, cx| {
          let anchor = this.bounds.get();
          let size = log_size(apps(cx).len());
          popup::open(anchor, size, window, cx, PrivacyLog::new);
        }),
      )
      .on_prepaint(move |b, _, _| bounds.set(b))
      .into_any_element()
  }
}

#[derive(Debug, PartialEq)]
struct AppAccess {
  name: String,
  kinds: Vec<(CaptureKind, bool)>,
  ended: Option<SystemTime>,
}

type Access = (CaptureKind, Option<String>, Option<SystemTime>);

fn by_app(log: impl IntoIterator<Item = Access>) -> Vec<AppAccess> {
  let mut apps: Vec<AppAccess> = Vec::new();
  for (kind, name, ended) in log {
    let name = name.unwrap_or_else(|| capture_label(kind).to_string());
    let ongoing = ended.is_none();
    let app = match apps.iter_mut().find(|a| a.name == name) {
      Some(app) => app,
      None => {
        apps.push(AppAccess {
          name,
          kinds: Vec::new(),
          ended,
        });
        apps.last_mut().expect("just pushed")
      }
    };
    match app.kinds.iter_mut().find(|(k, _)| *k == kind) {
      Some((_, active)) => *active |= ongoing,
      None => app.kinds.push((kind, ongoing)),
    }
    app.ended = match (app.ended, ended) {
      (Some(a), Some(b)) => Some(a.max(b)),
      _ => None,
    };
  }
  for app in &mut apps {
    app
      .kinds
      .sort_by_key(|(kind, _)| KINDS.iter().position(|k| k == kind));
  }
  apps.sort_by_key(|app| app.ended.map(std::cmp::Reverse));
  apps
}

fn apps(cx: &App) -> Vec<AppAccess> {
  let log = cx.pipewire().capture_log(cx);
  by_app(log.iter().map(|a| (a.kind, a.name.clone(), a.ended)))
}

fn log_size(entries: usize) -> gpui_kit::Size<Pixels> {
  let rows = entries.clamp(1, LOG_ROWS) as f32;
  popup::size(ROW + SEPARATOR + rows * ROW)
}

fn ago(time: SystemTime, now: SystemTime) -> String {
  let secs = now.duration_since(time).unwrap_or_default().as_secs();
  match secs {
    0..60 => "just now".to_string(),
    60..3600 => format!("{} min ago", secs / 60),
    3600..86400 => format!("{} h ago", secs / 3600),
    _ => format!("{} d ago", secs / 86400),
  }
}

struct PrivacyLog {
  focus: FocusHandle,
  _subscription: Subscription,
  _ticker: Task<()>,
}

impl PrivacyLog {
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let log = cx.pipewire().capture_log.clone();
    let subscription = cx.observe_in(&log, window, |_, _, window, cx| {
      window.resize(log_size(apps(cx).len()));
      cx.notify();
    });
    Self {
      focus: cx.focus_handle(),
      _subscription: subscription,
      _ticker: cx.ticker(Duration::from_secs(30), |_, cx| cx.notify()),
    }
  }

  fn entry(index: usize, app: &AppAccess, now: SystemTime, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let (active, idle) = (theme.primary, theme.muted_foreground);
    let (when, color) = match app.ended {
      None => ("Recording".to_string(), active),
      Some(ended) => (ago(ended, now), idle),
    };

    popup::row(SharedString::from(format!("access-{index}")), cx)
      .child(
        div()
          .flex()
          .gap_1()
          .children(app.kinds.iter().map(|&(kind, on)| {
            Icon::new(capture_icon(kind))
              .with_size(px(14.))
              .text_color(if on { active } else { idle })
          })),
      )
      .child(div().flex_1().min_w_0().truncate().child(app.name.clone()))
      .child(div().text_xs().text_color(color).child(when))
  }
}

impl Focusable for PrivacyLog {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for PrivacyLog {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let apps = apps(cx);
    let now = SystemTime::now();
    let muted = cx.theme().muted_foreground;

    popup::frame(self, cx)
      .child(
        popup::row("title", cx)
          .font_weight(gpui_kit::FontWeight::BOLD)
          .child("Recent access"),
      )
      .child(popup::separator(cx))
      .when(apps.is_empty(), |d| {
        d.child(
          popup::row("empty", cx)
            .text_color(muted)
            .child("Nothing recorded yet"),
        )
      })
      .children(
        apps
          .iter()
          .take(LOG_ROWS)
          .enumerate()
          .map(|(i, app)| Self::entry(i, app, now, cx)),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn one_row_per_app() {
    use CaptureKind::*;
    let at = |secs| Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs));
    let name = |n: &str| Some(n.to_string());
    // newest first, like the log
    let apps = by_app([
      (Microphone, name("Discord"), at(50)),
      (Screen, name("OBS"), None),
      (Microphone, name("Discord"), at(30)),
      (Screen, name("Discord"), at(40)),
      (Microphone, name("OBS"), at(10)),
    ]);
    assert_eq!(
      apps,
      [
        AppAccess {
          name: "OBS".into(),
          kinds: vec![(Microphone, false), (Screen, true)],
          ended: None,
        },
        AppAccess {
          name: "Discord".into(),
          kinds: vec![(Microphone, false), (Screen, false)],
          ended: at(50),
        },
      ]
    );
  }

  #[test]
  fn relative_time() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(100_000);
    let before = |secs| now - Duration::from_secs(secs);
    assert_eq!(ago(before(5), now), "just now");
    assert_eq!(ago(before(300), now), "5 min ago");
    assert_eq!(ago(before(7200), now), "2 h ago");
    assert_eq!(ago(before(90_000), now), "1 d ago");
  }
}
