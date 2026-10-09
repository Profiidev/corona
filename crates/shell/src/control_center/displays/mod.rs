use anyhow::Result;
use corona_components::components::card::{CardExt, ErrorCard};
use corona_compositor::{Compositor, CompositorExt, types::Monitor, types::MonitorChange};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AnyElement, App, AppContext, Context, Div, Entity, IntoElement, ParentElement, Render,
  SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::{Disableable, FocusableExt, IndexPath, StyledExt},
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    scroll::ScrollableElement,
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    tag::Tag,
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};
use rust_i18n::t;

type Names = Vec<SharedString>;

/// Monitors on and off, and one mirrored onto another
pub struct DisplaysPanel {
  source: Entity<SelectState<Names>>,
  target: Entity<SelectState<Names>>,
  /// What the user picked; until then the mirror that is on, or a sensible pair
  picked: (Option<String>, Option<String>),
  error: Option<String>,
  _subscriptions: Vec<Subscription>,
}

impl ControlCenterPanel for DisplaysPanel {
  const TYPE: ControlCenterType = ControlCenterType::Displays;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let source = cx.new(|cx| SelectState::new(Names::new(), None, window, cx));
    let target = cx.new(|cx| SelectState::new(Names::new(), None, window, cx));
    let monitors = cx.compositor().monitors.clone();

    let subscriptions = vec![
      cx.observe_in(&monitors, window, |this, _, window, cx| {
        this.sync_selects(window, cx);
        cx.notify();
      }),
      cx.subscribe_in(
        &source,
        window,
        |this, _, event: &SelectEvent<Names>, window, cx| {
          if let SelectEvent::Confirm(Some(name)) = event {
            this.picked.0 = Some(name.to_string());
            this.sync_selects(window, cx);
            cx.notify();
          }
        },
      ),
      cx.subscribe_in(
        &target,
        window,
        |this, _, event: &SelectEvent<Names>, window, cx| {
          if let SelectEvent::Confirm(Some(name)) = event {
            this.picked.1 = Some(name.to_string());
            this.sync_selects(window, cx);
            cx.notify();
          }
        },
      ),
    ];

    let mut panel = Self {
      source,
      target,
      picked: (None, None),
      error: None,
      _subscriptions: subscriptions,
    };
    panel.sync_selects(window, cx);
    panel
  }

  fn buttons(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![
      Button::new("displays-refresh")
        .icon(IconName::RefreshCw)
        .small()
        .cursor_pointer()
        .tooltip(t!("app.displays.refresh"))
        .on_click(cx.listener(|this, _, _, cx| {
          this.show(Compositor::refresh_monitors(cx), cx);
        }))
        .into_any_element(),
    ]
  }
}

/// The monitor `monitor` shows the content of, if it mirrors one. Hyprland
/// names it by id, or `none`.
pub(crate) fn mirror_source<'a>(monitor: &Monitor, all: &'a [Monitor]) -> Option<&'a Monitor> {
  let id: u32 = monitor.mirror_of.parse().ok()?;
  all.iter().find(|m| m.id == id && m.name != monitor.name)
}

/// Hyprland has no primary monitor; the one on at the origin is the closest
/// thing, else the first one on
pub(crate) fn primary(all: &[Monitor]) -> Option<&Monitor> {
  let on = || all.iter().filter(|m| !m.disabled);
  on().find(|m| m.x == 0 && m.y == 0).or_else(|| on().next())
}

/// The source and target to offer: the mirror that is on, else the primary
/// monitor onto the first other one that is on
pub(crate) fn default_pair(all: &[Monitor]) -> (Option<String>, Option<String>) {
  let mirroring = all
    .iter()
    .filter(|m| !m.disabled)
    .find_map(|m| Some((mirror_source(m, all)?, m)));
  if let Some((source, target)) = mirroring {
    return (Some(source.name.clone()), Some(target.name.clone()));
  }
  let source = primary(all).map(|m| m.name.clone());
  let target = all
    .iter()
    .find(|m| !m.disabled && Some(&m.name) != source.as_ref())
    .map(|m| m.name.clone());
  (source, target)
}

fn mode(monitor: &Monitor) -> String {
  format!(
    "{}×{} · {} Hz",
    monitor.width,
    monitor.height,
    monitor.refresh_rate.round()
  )
}

/// `2560×1440 · 144 Hz`, `Mirrors DP-1 · …` or `Disabled · …`
pub(crate) fn detail(monitor: &Monitor, all: &[Monitor]) -> String {
  if monitor.disabled {
    return t!("app.displays.disabled_detail", mode = mode(monitor)).into();
  }
  match mirror_source(monitor, all) {
    Some(source) => t!(
      "app.displays.mirrors_detail",
      source = source.name,
      mode = mode(monitor)
    )
    .into(),
    None => mode(monitor),
  }
}

fn card(cx: &App) -> Div {
  div().flex().flex_col().w_full().gap_2().p_2().card(cx)
}

impl DisplaysPanel {
  fn monitors(cx: &App) -> Vec<Monitor> {
    cx.compositor().list_monitors(cx).to_vec()
  }

  fn show(&mut self, result: Result<()>, cx: &mut Context<Self>) {
    if let Err(e) = result.log_err() {
      self.error = Some(e.to_string());
    }
    cx.notify();
  }

  /// The pair the selects show: a pick that still names a monitor that is on
  /// wins over the default
  fn pair(&self, all: &[Monitor]) -> (Option<String>, Option<String>) {
    let on = |name: &Option<String>| {
      name
        .clone()
        .filter(|n| all.iter().any(|m| !m.disabled && &m.name == n))
    };
    let (source, target) = default_pair(all);
    (on(&self.picked.0).or(source), on(&self.picked.1).or(target))
  }

  fn sync_selects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let all = Self::monitors(cx);
    let names: Names = all
      .iter()
      .filter(|m| !m.disabled)
      .map(|m| SharedString::from(m.name.clone()))
      .collect();
    let (source, target) = self.pair(&all);
    for (select, value) in [(&self.source, source), (&self.target, target)] {
      let index = value.and_then(|v| names.iter().position(|n| n.as_ref() == v));
      select.update(cx, |state, cx| {
        state.set_items(names.clone(), window, cx);
        state.set_selected_index(index.map(IndexPath::new), window, cx);
      });
    }
  }

  fn configure(&mut self, name: &str, change: MonitorChange, cx: &mut Context<Self>) {
    self.error = None;
    let result = Compositor::configure_monitor(name, change, cx);
    self.show(result, cx);
  }

  fn error(&self, cx: &Context<'_, Self>) -> Option<ErrorCard> {
    let error = self.error.clone()?;
    Some(
      ErrorCard::new("displays-error-dismiss", error).on_dismiss(cx.listener(|this, _, _, cx| {
        this.error = None;
        cx.notify();
      })),
    )
  }

  fn row(&self, monitor: &Monitor, all: &[Monitor], cx: &Context<'_, Self>) -> impl IntoElement {
    let theme = cx.theme();
    let mirrors = mirror_source(monitor, all).is_some();
    let is_primary = !mirrors && primary(all).is_some_and(|p| p.name == monitor.name);
    // the last monitor that is on stays on
    let last = !monitor.disabled && all.iter().filter(|m| !m.disabled).count() <= 1;
    let name = monitor.name.clone();

    div()
      .flex()
      .items_center()
      .w_full()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(Icon::new(IconName::Monitor).small())
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .gap_1()
          .child(
            div()
              .flex()
              .items_center()
              .gap_2()
              .child(div().text_sm().truncate().child(monitor.name.clone()))
              .when(is_primary, |d| {
                d.child(Tag::secondary().small().child(t!("app.displays.primary")))
              })
              .when(mirrors, |d| {
                d.child(
                  Tag::secondary().small().child(
                    div()
                      .flex()
                      .items_center()
                      .gap_1()
                      .child(Icon::new(IconName::Link).xsmall())
                      .child(t!("app.displays.mirror_tag")),
                  ),
                )
              }),
          )
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(detail(monitor, all)),
          ),
      )
      .child(
        Switch::new(SharedString::from(format!("display-{name}")))
          .checked(!monitor.disabled)
          .disabled(last)
          .on_click(cx.listener(move |this, checked: &bool, _, cx| {
            let change = match checked {
              true => MonitorChange::Enable,
              false => MonitorChange::Disable,
            };
            this.configure(&name, change, cx);
          })),
      )
  }

  fn mirror_card(&self, all: &[Monitor], cx: &Context<'_, Self>) -> impl IntoElement {
    let theme = cx.theme();
    let (source, target) = self.pair(all);
    let mirroring = target
      .as_ref()
      .and_then(|t| all.iter().find(|m| &m.name == t))
      .and_then(|t| mirror_source(t, all))
      .map(|m| m.name.clone());
    let can_mirror =
      matches!((&source, &target), (Some(s), Some(t)) if s != t) && mirroring != source;
    let label = |text| {
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(text)
    };
    let select = |state: &Entity<SelectState<Names>>| {
      Select::new(state)
        .w_full()
        .focus_ring(false)
        .cursor_pointer()
    };

    card(cx)
      .child(
        div()
          .flex()
          .flex_col()
          .child(div().text_sm().font_bold().child(t!("app.displays.mirror")))
          .child(label(t!("app.displays.mirror_description"))),
      )
      .child(label(t!("app.displays.source")))
      .child(select(&self.source))
      .child(label(t!("app.displays.target")))
      .child(select(&self.target))
      .child(
        div()
          .flex()
          .gap_2()
          .child(
            Button::new("displays-stop")
              .flex_1()
              .outline()
              .icon(IconName::Unlink)
              .label(t!("app.displays.stop_mirroring"))
              .cursor_pointer()
              .disabled(mirroring.is_none())
              .on_click(cx.listener({
                let target = target.clone();
                move |this, _, _, cx| {
                  if let Some(target) = &target {
                    this.configure(target, MonitorChange::StopMirroring, cx);
                  }
                }
              })),
          )
          .child(
            Button::new("displays-mirror")
              .flex_1()
              .primary()
              .icon(IconName::Link)
              .label(t!("app.displays.start_mirroring"))
              .cursor_pointer()
              .disabled(!can_mirror)
              .on_click(cx.listener(move |this, _, _, cx| {
                if let (Some(source), Some(target)) = (&source, &target) {
                  this.configure(target, MonitorChange::Mirror(source.clone()), cx);
                }
              })),
          ),
      )
  }
}

impl Render for DisplaysPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let all = Self::monitors(cx);

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .when_some(self.error(cx), |d, error| d.child(error))
      .child(
        // takes the height the mirror card leaves, so the panel is filled
        card(cx)
          .flex_1()
          .min_h_0()
          .child(
            div()
              .text_sm()
              .font_bold()
              .child(t!("app.displays.monitors")),
          )
          .child(
            div()
              .flex()
              .flex_col()
              .gap_2()
              .flex_1()
              .min_h_0()
              .overflow_y_scrollbar()
              .children(all.iter().map(|monitor| self.row(monitor, &all, cx))),
          ),
      )
      .child(self.mirror_card(&all, cx))
  }
}

#[cfg(test)]
mod tests {
  use corona_compositor::types::Workspace;

  use super::*;

  fn monitor(id: u32, name: &str, x: i32, disabled: bool, mirror_of: &str) -> Monitor {
    Monitor {
      id,
      name: name.into(),
      width: 1920,
      height: 1080,
      refresh_rate: 59.95,
      x,
      y: 0,
      active_scratchpad: None,
      active_workspace: Workspace {
        id: "1".into(),
        name: "1".into(),
        monitor: name.into(),
        monitor_id: id,
      },
      scale: 1.,
      focused: false,
      disabled,
      mirror_of: mirror_of.into(),
    }
  }

  /// DP-1 at the origin, HDMI-A-1 mirroring it, DP-2 off
  fn three() -> Vec<Monitor> {
    vec![
      monitor(0, "DP-1", 0, false, "none"),
      monitor(1, "HDMI-A-1", 2560, false, "0"),
      monitor(2, "DP-2", 4480, true, "none"),
    ]
  }

  #[test]
  fn mirrors_by_id() {
    let all = three();
    assert_eq!(mirror_source(&all[1], &all).unwrap().name, "DP-1");
    assert!(mirror_source(&all[0], &all).is_none());
    // an id no monitor has, or the monitor itself, is no mirror
    let stray = monitor(3, "X", 0, false, "9");
    assert!(mirror_source(&stray, &all).is_none());
    let own = monitor(0, "DP-1", 0, false, "0");
    assert!(mirror_source(&own, &all).is_none());
  }

  #[test]
  fn primary_is_at_the_origin_and_on() {
    let all = three();
    assert_eq!(primary(&all).unwrap().name, "DP-1");
    let off_origin = vec![
      monitor(0, "A", 0, true, "none"),
      monitor(1, "B", 1920, false, "none"),
    ];
    assert_eq!(primary(&off_origin).unwrap().name, "B");
    assert!(primary(&[]).is_none());
  }

  #[test]
  fn details() {
    let all = three();
    assert_eq!(detail(&all[0], &all), "1920×1080 · 60 Hz");
    assert!(detail(&all[1], &all).contains("DP-1"));
    assert!(detail(&all[2], &all).contains("1920×1080"));
    assert_ne!(detail(&all[2], &all), detail(&all[0], &all));
  }

  #[test]
  fn default_pair_follows_the_mirror_or_the_primary() {
    assert_eq!(
      default_pair(&three()),
      (Some("DP-1".into()), Some("HDMI-A-1".into()))
    );
    let plain = vec![
      monitor(0, "DP-1", 0, false, "none"),
      monitor(1, "DP-2", 2560, true, "none"),
      monitor(2, "HDMI-A-1", 4480, false, "none"),
    ];
    assert_eq!(
      default_pair(&plain),
      (Some("DP-1".into()), Some("HDMI-A-1".into()))
    );
    assert_eq!(default_pair(&plain[..1]), (Some("DP-1".into()), None));
  }

  #[gpui_kit::test]
  fn renders_and_changes_monitors(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    use crate::test_support::{FakeCompositor, setup};

    let fake = setup(
      FakeCompositor {
        monitors: three(),
        ..Default::default()
      },
      cx,
    );
    let (panel, cx) = cx.add_window_view(DisplaysPanel::init);
    cx.update(|window, cx| window.render_frame(cx));

    // the selects follow the mirror that is on
    let picked = |cx: &mut gpui_kit::VisualTestContext| {
      cx.update(|_, cx| {
        let panel = panel.read(cx);
        let value = |s: &Entity<SelectState<Names>>| s.read(cx).selected_value().cloned();
        (value(&panel.source), value(&panel.target))
      })
    };
    assert_eq!(picked(cx), (Some("DP-1".into()), Some("HDMI-A-1".into())));

    panel.update(cx, |panel, cx| {
      panel.configure("HDMI-A-1", MonitorChange::StopMirroring, cx);
      panel.configure("DP-2", MonitorChange::Enable, cx);
    });
    assert_eq!(
      *fake.calls.borrow(),
      ["monitor HDMI-A-1 StopMirroring", "monitor DP-2 Enable"]
    );
    assert!(panel.read_with(cx, |panel, _| panel.error.is_none()));
    cx.update(|window, cx| window.render_frame(cx));
  }
}
