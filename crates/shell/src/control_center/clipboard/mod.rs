//! The clipboard history: search, type filter, pins; a click copies an entry again

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use corona_clipboard::{ClipboardExt, Entry, Kind};
use corona_components::components::{card::CardExt, modal::modal};
use corona_launcher::{
  command::Hud,
  commands::clipboard::{clip, kind_label, swatch},
};
use corona_utils::{error::ErrorLogExt, ticker::TickerExt};
use gpui_kit::{
  AnyElement, App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement,
  Render, SharedString, StatefulInteractiveElement, Styled, Subscription, Task, TestSupportExt,
  Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariant, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
  },
  div, img,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

use crate::control_center::{ControlCenterPanel, notifications::ago, variants::ControlCenterType};

const LIMIT: usize = 100;
const TICK: Duration = Duration::from_secs(30);
const THUMBNAIL: f32 = 32.;
const KINDS: [Option<Kind>; 5] = [
  None,
  Some(Kind::Text),
  Some(Kind::Image),
  Some(Kind::Link),
  Some(Kind::Color),
];

pub struct ClipboardPanel {
  query: Entity<InputState>,
  kind: Option<Kind>,
  confirm_clear: bool,
  _subscriptions: Vec<Subscription>,
  _ticker: Task<()>,
}

impl ControlCenterPanel for ClipboardPanel {
  const TYPE: ControlCenterType = ControlCenterType::Clipboard;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let query =
      cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.launcher.clipboard.search")));
    let mut subscriptions =
      vec![
        cx.subscribe_in(&query, window, |_, _, event: &InputEvent, _, cx| {
          if let InputEvent::Change = event {
            cx.notify();
          }
        }),
      ];
    if let Some(history) = cx.clipboard_history() {
      let revision = history.revision.clone();
      subscriptions.push(cx.observe(&revision, |_, _, cx| cx.notify()));
    }
    Self {
      query,
      kind: None,
      confirm_clear: false,
      _subscriptions: subscriptions,
      _ticker: cx.ticker(TICK, |_, cx| cx.notify()),
    }
  }

  fn buttons(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    let empty = cx
      .clipboard_history()
      .is_none_or(|h| h.list("", None, 1).is_empty());
    vec![
      Button::new("clipboard-clear")
        .icon(IconName::Trash)
        .tooltip(t!("app.launcher.clipboard.clear"))
        .cursor_pointer()
        .with_variant(ButtonVariant::Danger)
        .disabled(empty)
        .on_click(cx.listener(|this, _, _, cx| {
          this.confirm_clear = true;
          cx.notify();
        }))
        .into_any_element(),
    ]
  }
}

/// Copies the whole entry and moves it to the top
fn copy(id: i64, cx: &mut App) {
  let Some(history) = cx.clipboard_history().cloned() else {
    return;
  };
  let Some(entry) = history.get(id) else {
    return;
  };
  if let Ok(clip) = clip(&entry).log_err() {
    corona_launcher::paste::copy(&clip, cx);
    corona_launcher::ui::hud::show(Hud::new(t!("app.launcher.hud.copied")), cx);
    let _ = history.touch(id, cx).log_err();
  }
}

fn used(entry: &Entry) -> String {
  let used = UNIX_EPOCH + Duration::from_secs(entry.used);
  ago(SystemTime::now().duration_since(used).unwrap_or_default())
}

fn title(entry: &Entry) -> SharedString {
  match &entry.text {
    Some(text) => text
      .lines()
      .find(|l| !l.trim().is_empty())
      .unwrap_or_default()
      .chars()
      .take(120)
      .collect::<String>()
      .into(),
    None => t!(
      "app.launcher.clipboard.image_title",
      size = entry.size / 1024
    )
    .into(),
  }
}

fn preview(entry: &Entry) -> AnyElement {
  let frame = div()
    .size(px(THUMBNAIL))
    .flex_none()
    .flex()
    .items_center()
    .justify_center()
    .rounded_md()
    .overflow_hidden();
  match (&entry.path, entry.text.as_deref().and_then(swatch)) {
    (Some(path), _) => frame.child(img(path.clone()).size_full()),
    (None, Some(color)) => frame.child(div().size_6().rounded_md().bg(color)),
    _ => frame.child(
      Icon::new(match entry.kind {
        Kind::Link => IconName::Link,
        _ => IconName::Type,
      })
      .small(),
    ),
  }
  .into_any_element()
}

impl ClipboardPanel {
  fn filters(&self, cx: &Context<Self>) -> impl IntoElement {
    div().flex().gap_1().children(KINDS.map(|kind| {
      let (id, label) = match kind {
        Some(kind) => (
          format!("clipboard-kind-{kind:?}").to_lowercase(),
          kind_label(kind),
        ),
        None => (
          "clipboard-kind-all".to_string(),
          t!("app.launcher.clipboard.all").into(),
        ),
      };
      Button::new(SharedString::from(id))
        .label(label)
        .small()
        .cursor_pointer()
        .when(kind == self.kind, |b| b.primary())
        .on_click(cx.listener(move |this, _, _, cx| {
          this.kind = kind;
          cx.notify();
        }))
    }))
  }

  fn row(entry: &Entry, cx: &App) -> impl IntoElement {
    let (id, pinned) = (entry.id, entry.pinned);
    div()
      .id(SharedString::from(format!("clip-{id}")))
      .test_support()
      .flex()
      .items_center()
      .gap_2()
      .p_2()
      .card(cx)
      .cursor_pointer()
      .on_click(move |_, _, cx| copy(id, cx))
      .child(preview(entry))
      .child(
        div()
          .flex_1()
          .min_w_0()
          .truncate()
          .text_sm()
          .child(title(entry)),
      )
      .child(
        div()
          .flex_none()
          .text_xs()
          .text_color(cx.theme().colors.muted_foreground)
          .child(used(entry)),
      )
      .child(
        Button::new(SharedString::from(format!("clip-{id}-pin")))
          .icon(match pinned {
            true => IconName::PinOff,
            false => IconName::Pin,
          })
          .small()
          .with_variant(ButtonVariant::Ghost)
          .when(pinned, |b| b.primary())
          .tooltip(match pinned {
            true => t!("app.launcher.clipboard.unpin"),
            false => t!("app.launcher.clipboard.pin"),
          })
          .cursor_pointer()
          .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if let Some(history) = cx.clipboard_history().cloned() {
              let _ = history.set_pinned(id, !pinned, cx).log_err();
            }
          }),
      )
      .child(
        Button::new(SharedString::from(format!("clip-{id}-delete")))
          .icon(IconName::Trash)
          .small()
          .with_variant(ButtonVariant::Ghost)
          .tooltip(t!("app.launcher.clipboard.delete"))
          .cursor_pointer()
          .on_click(move |_, _, cx| {
            cx.stop_propagation();
            if let Some(history) = cx.clipboard_history().cloned() {
              let _ = history.delete(id, cx).log_err();
            }
          }),
      )
  }

  fn confirm(&self, cx: &Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    modal(
      theme,
      div()
        .child(
          div()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::BOLD)
            .child(t!("app.launcher.clipboard.clear")),
        )
        .child(
          div()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(t!("app.launcher.clipboard.clear_message")),
        )
        .child(
          div()
            .flex()
            .justify_end()
            .gap_2()
            .child(
              Button::new("clipboard-clear-cancel")
                .label(t!("app.common.cancel"))
                .small()
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                  this.confirm_clear = false;
                  cx.notify();
                })),
            )
            .child(
              Button::new("clipboard-clear-confirm")
                .label(t!("app.launcher.clipboard.clear_button"))
                .small()
                .with_variant(ButtonVariant::Danger)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, cx| {
                  this.confirm_clear = false;
                  if let Some(history) = cx.clipboard_history().cloned() {
                    let _ = history.clear(cx).log_err();
                  }
                  cx.notify();
                })),
            ),
        ),
      cx.listener(|this, _, _, cx| {
        this.confirm_clear = false;
        cx.notify();
      }),
    )
  }
}

impl Render for ClipboardPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let muted = |text: SharedString, cx: &App| {
      div().flex().justify_center().p_2().card(cx).child(
        div()
          .text_xs()
          .text_color(cx.theme().colors.muted_foreground)
          .child(text),
      )
    };
    let Some(history) = cx.clipboard_history() else {
      return div()
        .size_full()
        .child(muted(t!("app.launcher.clipboard.off").into(), cx))
        .into_any_element();
    };
    let entries = history.list(&self.query.read(cx).value(), self.kind, LIMIT);

    div()
      .relative()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(Input::new(&self.query).small().cleanable(true))
      .child(self.filters(cx))
      .when_else(
        entries.is_empty(),
        |d| d.child(muted(t!("app.launcher.clipboard.empty").into(), cx)),
        |d| {
          d.child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_h_0()
              .gap_2()
              .overflow_y_scrollbar()
              .children(entries.iter().map(|e| Self::row(e, cx))),
          )
        },
      )
      .when(self.confirm_clear, |d| d.child(self.confirm(cx)))
      .into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use corona_clipboard::{ClipboardHistory, Store};
  use gpui_kit::{self as gpui, TestAppContext, WindowOptions, test::TestWindowExt};

  use super::*;

  #[gpui::test]
  fn lists_and_filters(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let text = store.add_text("hello world", "text/plain").unwrap();
    let link = store.add_text("https://example.com", "text/plain").unwrap();
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      ClipboardHistory::install(store, cx);
    });
    let (window, panel) = cx.update(|cx| {
      let mut panel = None;
      let window = gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
        let view = cx.new(|cx| ClipboardPanel::init(window, cx));
        panel = Some(view.clone());
        view
      })
      .unwrap()
      .0;
      (window, panel.unwrap())
    });
    let shown = |cx: &mut TestAppContext| {
      cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        [text, link].map(|id| {
          window
            .try_find(SharedString::from(format!("clip-{id}")))
            .is_some()
        })
      })
      .unwrap()
    };
    assert_eq!(shown(cx), [true, true]);
    cx.update(|cx| panel.update(cx, |p, _| p.kind = Some(Kind::Link)));
    assert_eq!(shown(cx), [false, true]);
  }
}
