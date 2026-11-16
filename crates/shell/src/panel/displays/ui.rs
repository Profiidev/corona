//! The per-monitor settings dialog and the layout view

use corona_compositor::{
  Compositor,
  types::{Mode, Monitor, MonitorChange, logical_size},
};
use gpui_kit::{
  AnyElement, App, AppContext, Context, DispatchPhase, Entity, InteractiveElement, IntoElement,
  MouseButton, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, SharedString, Styled,
  Subscription, Window,
  assets::IconName,
  base::{
    Disableable, FocusableExt, IndexPath, StyledExt,
    input::{InputEvent, InputState},
    slider::{SliderEvent, SliderState},
  },
  canvas,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    input::Input,
    select::{Select, SelectEvent, SelectState},
    slider::Slider,
  },
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

use super::{
  DisplaysPanel, Names,
  layout::{self, Rect, View},
  settings,
};

const MAP_W: f32 = 466.;
const MAP_H: f32 = 270.;

fn size_label(size: (u32, u32)) -> SharedString {
  format!("{}×{}", size.0, size.1).into()
}

fn rate_label(rate: f32) -> SharedString {
  match (rate - rate.round()).abs() < 0.005 {
    true => format!("{} Hz", rate.round()),
    false => format!("{rate:.2} Hz"),
  }
  .into()
}

fn select(
  items: Names,
  selected: Option<usize>,
  window: &mut Window,
  cx: &mut App,
) -> Entity<SelectState<Names>> {
  cx.new(|cx| SelectState::new(items, selected.map(IndexPath::new), window, cx))
}

fn label(text: impl Into<SharedString>, cx: &App) -> AnyElement {
  div()
    .text_xs()
    .text_color(cx.theme().colors.muted_foreground)
    .child(text.into())
    .into_any_element()
}

/// Resolution, refresh rate and scale of one monitor, not yet applied
pub(super) struct Dialog {
  name: String,
  modes: Vec<Mode>,
  transform: i32,
  physical_mm: (u32, u32),
  size: (u32, u32),
  rate: f32,
  percent: f32,
  res: Entity<SelectState<Names>>,
  hz: Entity<SelectState<Names>>,
  slider: Entity<SliderState>,
  input: Entity<InputState>,
  _subscriptions: Vec<Subscription>,
}

impl Dialog {
  fn mode(&self) -> Option<Mode> {
    settings::mode_for(&self.modes, self.size, self.rate)
  }

  fn logical(&self) -> (i32, i32) {
    logical_size(
      self.size,
      settings::scale_from_percent(self.percent),
      self.transform,
    )
  }
}

/// A monitor in the layout view
pub(super) struct Item {
  name: String,
  pub(super) rect: Rect,
  base: (i32, i32),
  physical_mm: (u32, u32),
}

struct Drag {
  start_pointer: Point<Pixels>,
  start_pos: (i32, i32),
}

/// Monitor positions being edited
pub(super) struct LayoutDraft {
  pub(super) items: Vec<Item>,
  view: View,
  sel: usize,
  drag: Option<Drag>,
  select: Entity<SelectState<Names>>,
  x: Entity<InputState>,
  y: Entity<InputState>,
  _subscriptions: Vec<Subscription>,
}

impl LayoutDraft {
  fn rects(&self) -> Vec<Rect> {
    self.items.iter().map(|i| i.rect).collect()
  }

  pub(super) fn overlapping(&self) -> Vec<usize> {
    layout::overlapping(&self.rects())
  }

  pub(super) fn unreachable(&self) -> Vec<usize> {
    layout::unreachable(&self.rects())
  }
}

/// Monitors the layout can place: on and not mirroring another
pub(super) fn positionable(all: &[Monitor]) -> Vec<&Monitor> {
  all
    .iter()
    .filter(|m| !m.disabled && super::mirror_source(m, all).is_none())
    .collect()
}

impl DisplaysPanel {
  pub(super) fn open_dialog(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
    let Some(m) = Self::monitors(cx)
      .into_iter()
      .find(|m| m.name == name && !m.disabled)
    else {
      return;
    };
    let modes = m.hardware.modes.clone();
    let size = (m.width, m.height);
    let rate = m.refresh_rate;
    let percent = settings::percent(m.scale);

    let sizes: Names = settings::resolutions(&modes)
      .into_iter()
      .map(size_label)
      .collect();
    let res_index = sizes.iter().position(|s| *s == size_label(size));
    let res = select(sizes, res_index, window, cx);
    let hz = select(Names::new(), None, window, cx);
    let slider = cx.new(|_| {
      SliderState::new()
        .min(settings::MIN_PERCENT)
        .max(settings::MAX_PERCENT)
        .step(5.)
        .default_value(percent)
    });
    let input = cx.new(|cx| InputState::new(window, cx).default_value(percent.to_string()));

    let subscriptions = vec![
      cx.subscribe_in(
        &res,
        window,
        |this, _, event: &SelectEvent<Names>, window, cx| {
          let SelectEvent::Confirm(Some(value)) = event else {
            return;
          };
          let Some(d) = this.dialog.as_mut() else {
            return;
          };
          if let Some(size) = settings::resolutions(&d.modes)
            .into_iter()
            .find(|s| size_label(*s) == *value)
          {
            d.size = size;
            if let Some(mode) = d.mode() {
              d.rate = mode.refresh_rate;
            }
          }
          this.sync_rates(window, cx);
          cx.notify();
        },
      ),
      cx.subscribe_in(&hz, window, |this, _, event: &SelectEvent<Names>, _, cx| {
        let SelectEvent::Confirm(Some(value)) = event else {
          return;
        };
        if let Some(d) = this.dialog.as_mut()
          && let Some(mode) = settings::rates(&d.modes, d.size)
            .into_iter()
            .find(|m| rate_label(m.refresh_rate) == *value)
        {
          d.rate = mode.refresh_rate;
        }
        cx.notify();
      }),
      cx.subscribe_in(
        &slider,
        window,
        |this, _, event: &SliderEvent, window, cx| {
          let SliderEvent::Change(value) = event else {
            return;
          };
          this.set_percent(value.start(), true, window, cx);
        },
      ),
      cx.subscribe_in(
        &input,
        window,
        |this, input, event: &InputEvent, window, cx| {
          if matches!(event, InputEvent::Change)
            && let Ok(percent) = input.read(cx).value().trim().parse::<f32>()
          {
            this.set_percent(percent, false, window, cx);
          }
        },
      ),
    ];

    self.dialog = Some(Dialog {
      name: m.name,
      modes,
      transform: m.hardware.transform,
      physical_mm: m.hardware.physical_mm,
      size,
      rate,
      percent,
      res,
      hz,
      slider,
      input,
      _subscriptions: subscriptions,
    });
    self.sync_rates(window, cx);
    cx.notify();
  }

  /// Lists the rates of the chosen resolution and selects the one in use
  fn sync_rates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(d) = &self.dialog else {
      return;
    };
    let rates = settings::rates(&d.modes, d.size);
    let items: Names = rates.iter().map(|m| rate_label(m.refresh_rate)).collect();
    let index = d
      .mode()
      .and_then(|m| rates.iter().position(|r| r.refresh_rate == m.refresh_rate));
    d.hz.update(cx, |state, cx| {
      state.set_items(items, window, cx);
      state.set_selected_index(index.map(IndexPath::new), window, cx);
    });
  }

  /// A scale from the slider or the number field; the other one follows
  pub(super) fn set_percent(
    &mut self,
    percent: f32,
    from_slider: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(d) = self.dialog.as_mut() else {
      return;
    };
    let percent = percent.clamp(settings::MIN_PERCENT, settings::MAX_PERCENT);
    if d.percent == percent {
      return;
    }
    d.percent = percent;
    if from_slider {
      d.input.update(cx, |input, cx| {
        input.set_value(percent.round().to_string(), window, cx)
      });
    } else {
      d.slider
        .update(cx, |slider, cx| slider.set_value(percent, window, cx));
    }
    cx.notify();
  }

  pub(super) fn close_dialog(&mut self, cx: &mut Context<Self>) {
    self.dialog = None;
    cx.notify();
  }

  pub(super) fn apply_dialog(&mut self, cx: &mut Context<Self>) {
    let Some(d) = self.dialog.take() else {
      return;
    };
    let change = MonitorChange::Set {
      mode: d.mode(),
      scale: Some(settings::scale_from_percent(d.percent)),
      position: None,
    };
    self.configure(&d.name, change, cx);
  }

  pub(super) fn dialog_view(&self, cx: &Context<Self>) -> Option<AnyElement> {
    let d = self.dialog.as_ref()?;
    let theme = cx.theme();
    let (lw, lh) = d.logical();
    let physical = match (d.physical_mm, self.monitor_dpi(&d.name, cx)) {
      ((w, h), Some(dpi)) if w > 0 => {
        Some(t!("app.displays.physical", w = w, h = h, dpi = dpi.round()).to_string())
      }
      _ => None,
    };
    let field = |title: String, body: AnyElement| {
      div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_1()
        .child(label(title, cx))
        .child(body)
    };

    let card = div()
      .flex()
      .flex_col()
      .gap_3()
      .p_3()
      .w_full()
      .max_w(px(440.))
      .rounded_xl()
      .border_1()
      .border_color(theme.colors.border)
      .bg(theme.colors.background)
      .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
      .child(div().text_sm().font_bold().child(d.name.clone()))
      .child(
        div()
          .flex()
          .gap_3()
          .child(field(
            t!("app.displays.resolution").to_string(),
            Select::new(&d.res)
              .w_full()
              .focus_ring(false)
              .cursor_pointer()
              .into_any_element(),
          ))
          .child(field(
            t!("app.displays.refresh_rate").to_string(),
            Select::new(&d.hz)
              .w_full()
              .focus_ring(false)
              .cursor_pointer()
              .into_any_element(),
          )),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .child(
            div()
              .flex()
              .justify_between()
              .child(label(t!("app.displays.scale").to_string(), cx))
              .child(label(
                t!("app.displays.logical_size", w = lw, h = lh).to_string(),
                cx,
              )),
          )
          .child(
            div()
              .flex()
              .items_center()
              .gap_3()
              .child(div().flex_1().child(Slider::new(&d.slider)))
              .child(
                div()
                  .w(px(84.))
                  .child(Input::new(&d.input).suffix(label("%", cx))),
              ),
          ),
      )
      .when_some(physical, |this, text| this.child(label(text, cx)))
      .child(
        div()
          .flex()
          .gap_2()
          .child(
            Button::new("displays-dialog-cancel")
              .flex_1()
              .outline()
              .label(t!("app.displays.cancel"))
              .cursor_pointer()
              .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
          )
          .child(
            Button::new("displays-dialog-apply")
              .flex_1()
              .primary()
              .label(t!("app.displays.apply"))
              .cursor_pointer()
              .on_click(cx.listener(|this, _, _, cx| this.apply_dialog(cx))),
          ),
      );

    Some(
      div()
        .id("displays-dialog")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .p_3()
        .bg(theme.colors.background.opacity(0.7))
        .on_mouse_down(
          MouseButton::Left,
          cx.listener(|this, _, _, cx| this.close_dialog(cx)),
        )
        .child(card)
        .into_any_element(),
    )
  }

  fn monitor_dpi(&self, name: &str, cx: &App) -> Option<f32> {
    Self::monitors(cx)
      .iter()
      .find(|m| m.name == name)
      .and_then(Monitor::dpi)
  }

  // ---- layout ----

  pub(super) fn open_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let all = Self::monitors(cx);
    let items: Vec<Item> = positionable(&all)
      .into_iter()
      .map(|m| {
        let (w, h) = m.logical_size();
        Item {
          name: m.name.clone(),
          rect: Rect {
            x: m.x,
            y: m.y,
            w,
            h,
          },
          base: (m.x, m.y),
          physical_mm: m.hardware.physical_mm,
        }
      })
      .collect();
    if items.len() < 2 {
      return;
    }
    let rects: Vec<Rect> = items.iter().map(|i| i.rect).collect();
    let names: Names = items.iter().map(|i| i.name.clone().into()).collect();
    let select = select(names, Some(0), window, cx);
    let x = cx.new(|cx| InputState::new(window, cx));
    let y = cx.new(|cx| InputState::new(window, cx));

    let subscriptions = vec![
      cx.subscribe_in(
        &select,
        window,
        |this, _, event: &SelectEvent<Names>, window, cx| {
          let SelectEvent::Confirm(Some(name)) = event else {
            return;
          };
          if let Some(l) = this.layout.as_mut()
            && let Some(i) = l.items.iter().position(|i| i.name == name.as_ref())
          {
            l.sel = i;
          }
          this.sync_inputs(window, cx);
          cx.notify();
        },
      ),
      cx.subscribe_in(&x, window, |this, input, event: &InputEvent, _, cx| {
        if matches!(event, InputEvent::Change) {
          this.type_position(input.read(cx).value().to_string(), true, cx);
        }
      }),
      cx.subscribe_in(&y, window, |this, input, event: &InputEvent, _, cx| {
        if matches!(event, InputEvent::Change) {
          this.type_position(input.read(cx).value().to_string(), false, cx);
        }
      }),
    ];

    self.layout = Some(LayoutDraft {
      items,
      view: View::new(&rects, MAP_W, MAP_H),
      sel: 0,
      drag: None,
      select,
      x,
      y,
      _subscriptions: subscriptions,
    });
    self.sync_inputs(window, cx);
    cx.notify();
  }

  pub(super) fn close_layout(&mut self, cx: &mut Context<Self>) {
    self.layout = None;
    cx.notify();
  }

  pub(super) fn reset_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(l) = self.layout.as_mut() {
      for i in &mut l.items {
        (i.rect.x, i.rect.y) = i.base;
      }
    }
    self.sync_inputs(window, cx);
    cx.notify();
  }

  pub(super) fn apply_layout(&mut self, cx: &mut Context<Self>) {
    let Some(l) = self.layout.take() else {
      return;
    };
    self.error.clear();
    for i in l.items.iter().filter(|i| (i.rect.x, i.rect.y) != i.base) {
      let change = MonitorChange::Set {
        mode: None,
        scale: None,
        position: Some((i.rect.x, i.rect.y)),
      };
      if let Err(e) = Compositor::configure_monitor(&i.name, change, cx) {
        self.error.set(Err(e));
        break;
      }
    }
    cx.notify();
  }

  fn sync_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(l) = &self.layout else {
      return;
    };
    let r = l.items[l.sel].rect;
    let name = l.items[l.sel].name.clone();
    for (input, value) in [(&l.x, r.x), (&l.y, r.y)] {
      let text = value.to_string();
      if input.read(cx).value() != text.as_str() {
        input.update(cx, |input, cx| input.set_value(text, window, cx));
      }
    }
    l.select.update(cx, |state, cx| {
      state.set_selected_value(&name.into(), window, cx);
    });
  }

  fn type_position(&mut self, text: String, horizontal: bool, cx: &mut Context<Self>) {
    let (Some(l), Ok(v)) = (self.layout.as_mut(), text.trim().parse::<i32>()) else {
      return;
    };
    let rect = &mut l.items[l.sel].rect;
    match horizontal {
      true => rect.x = v,
      false => rect.y = v,
    }
    cx.notify();
  }

  fn on_move(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
    let Some(l) = self.layout.as_mut() else {
      return;
    };
    let Some(drag) = &l.drag else {
      return;
    };
    let others: Vec<Rect> = l
      .items
      .iter()
      .enumerate()
      .filter(|(i, _)| *i != l.sel)
      .map(|(_, i)| i.rect)
      .collect();
    let me = l.items[l.sel].rect;
    let dx = f32::from(position.x - drag.start_pointer.x);
    let dy = f32::from(position.y - drag.start_pointer.y);
    let (x, y) = layout::drag(&me, drag.start_pos, dx, dy, l.view.k, &others);
    let rect = &mut l.items[l.sel].rect;
    if (rect.x, rect.y) != (x, y) {
      (rect.x, rect.y) = (x, y);
      self.sync_inputs(window, cx);
      cx.notify();
    }
  }

  fn on_release(&mut self, cx: &mut Context<Self>) {
    if let Some(l) = self.layout.as_mut()
      && l.drag.take().is_some()
    {
      cx.notify();
    }
  }

  pub(super) fn layout_view(&self, cx: &mut Context<Self>) -> AnyElement {
    let Some(l) = &self.layout else {
      return div().into_any_element();
    };
    let theme = cx.theme();
    let overlapping = l.overlapping();
    let unreachable = l.unreachable();
    let all = Self::monitors(cx);
    let mirrors: Vec<String> = all
      .iter()
      .filter(|m| !m.disabled)
      .filter_map(|m| {
        Some(
          t!(
            "app.displays.mirrors_note",
            name = m.name,
            source = super::mirror_source(m, &all)?.name
          )
          .to_string(),
        )
      })
      .collect();
    let warning = if !overlapping.is_empty() {
      Some(t!("app.displays.overlap").to_string())
    } else if !unreachable.is_empty() {
      Some(t!("app.displays.not_connected").to_string())
    } else {
      None
    };

    let entity = cx.entity();
    let pointer = canvas(
      |_, _, _| (),
      move |_, _, window, _| {
        let moved = entity.clone();
        window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
          if phase == DispatchPhase::Bubble {
            moved.update(cx, |this, cx| this.on_move(e.position, window, cx));
          }
        });
        let released = entity.clone();
        window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
          if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
            released.update(cx, |this, cx| this.on_release(cx));
          }
        });
      },
    )
    .absolute()
    .size_full();

    let rects = l.items.iter().enumerate().map(|(index, item)| {
      let (x, y, w, h) = l.view.to_map(&item.rect);
      let selected = index == l.sel;
      let border = match (overlapping.contains(&index), unreachable.contains(&index)) {
        (true, _) => theme.colors.danger,
        (_, true) => theme.colors.warning,
        _ => theme.colors.border,
      };
      let mm = match item.physical_mm {
        (w, h) if w > 0 => format!(" · {w}×{h} mm"),
        _ => String::new(),
      };
      div()
        .id(SharedString::from(format!("displays-map-{}", item.name)))
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(w))
        .h(px(h))
        .overflow_hidden()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .border_color(border)
        .cursor_grab()
        .text_xs()
        .when(selected, |d| {
          d.bg(theme.colors.primary)
            .text_color(theme.colors.primary_foreground)
        })
        .when(!selected, |d| {
          d.bg(theme.colors.muted)
            .text_color(theme.colors.muted_foreground)
        })
        .child(div().font_bold().child(item.name.clone()))
        .child(
          div()
            .opacity(0.75)
            .child(format!("{}×{}{mm}", item.rect.w, item.rect.h)),
        )
        .on_mouse_down(
          MouseButton::Left,
          cx.listener(move |this, e: &gpui_kit::MouseDownEvent, window, cx| {
            if let Some(l) = this.layout.as_mut() {
              l.sel = index;
              l.drag = Some(Drag {
                start_pointer: e.position,
                start_pos: (l.items[index].rect.x, l.items[index].rect.y),
              });
            }
            this.sync_inputs(window, cx);
            cx.notify();
          }),
        )
    });

    let blocked = !overlapping.is_empty();
    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(
        div()
          .flex()
          .items_center()
          .gap_2()
          .child(
            Button::new("displays-layout-back")
              .icon(IconName::ArrowLeft)
              .small()
              .outline()
              .cursor_pointer()
              .on_click(cx.listener(|this, _, _, cx| this.close_layout(cx))),
          )
          .child(
            div()
              .flex()
              .flex_col()
              .child(div().text_sm().font_bold().child(t!("app.displays.layout")))
              .child(label(t!("app.displays.layout_hint").to_string(), cx)),
          ),
      )
      .child(
        div()
          .relative()
          .w(px(MAP_W))
          .h(px(MAP_H))
          .overflow_hidden()
          .rounded_md()
          .border_1()
          .border_color(theme.colors.border)
          .bg(theme.colors.background)
          .child(pointer)
          .children(rects),
      )
      .child(label(mirrors.join(" · "), cx))
      .child(
        div()
          .flex()
          .gap_3()
          .child(
            div().flex_1().child(
              Select::new(&l.select)
                .w_full()
                .focus_ring(false)
                .cursor_pointer(),
            ),
          )
          .child(
            div()
              .w(px(90.))
              .child(Input::new(&l.x).prefix(label("X", cx))),
          )
          .child(
            div()
              .w(px(90.))
              .child(Input::new(&l.y).prefix(label("Y", cx))),
          ),
      )
      .child(
        div()
          .flex()
          .items_center()
          .gap_2()
          .child(
            div()
              .flex_1()
              .text_xs()
              .text_color(theme.colors.danger)
              .child(warning.unwrap_or_default()),
          )
          .child(
            Button::new("displays-layout-reset")
              .outline()
              .label(t!("app.displays.reset"))
              .cursor_pointer()
              .on_click(cx.listener(|this, _, window, cx| this.reset_layout(window, cx))),
          )
          .child(
            Button::new("displays-layout-apply")
              .primary()
              .label(t!("app.displays.apply"))
              .cursor_pointer()
              .disabled(blocked)
              .on_click(cx.listener(|this, _, _, cx| this.apply_layout(cx))),
          ),
      )
      .into_any_element()
  }
}
