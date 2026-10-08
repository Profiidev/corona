//! Setting rows bound to the [`Config`]: each reads `cx.config()` and writes
//! through [`corona_config::update`], so a change lands in the settings file and
//! every part of the shell at once.
//!
//! Text and number fields commit on enter or when they lose focus rather than on
//! every keystroke: some sections reopen windows when they change (the bars,
//! wallpaper, taskbar).

use corona_config::{Config, ConfigProvider};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Entity, ParentElement, SharedString, Styled, Subscription, Window,
  component::{
    ActiveTheme, Sizable,
    input::{Input, InputEvent, InputState, NumberInput},
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectEvent, SelectState},
    setting::{RenderOptions, SettingField},
    slider::{Slider, SliderEvent, SliderState},
  },
  div,
  prelude::FluentBuilder,
  px,
};
use gpui_kit::{
  Focusable,
  base::{AxisExt, Disableable},
};
use rust_i18n::t;
use serde::{Serialize, de::DeserializeOwned};
use std::{borrow::Cow, fmt::Display};

/// Reads a setting; also called on `Config::default()` for its default
pub(super) trait Get<T>: Fn(&Config) -> T + Clone + 'static {}
impl<T, F: Fn(&Config) -> T + Clone + 'static> Get<T> for F {}

/// Writes a setting
pub(super) trait Set<T>: Fn(&mut Config, T) + Clone + 'static {}
impl<T, F: Fn(&mut Config, T) + Clone + 'static> Set<T> for F {}

/// Changes the settings, logging what went wrong
pub(super) fn save(cx: &mut App, edit: impl FnOnce(&mut Config)) {
  let _ = corona_config::update(cx, edit).log_err();
}

fn is_default<T: PartialEq>(get: impl Get<T>) -> impl Fn(&App) -> bool {
  move |cx| get(cx.config()) != get(&Config::default())
}

fn reset<T>(get: impl Get<T>, set: impl Set<T>) -> impl Fn(&mut Window, &mut App) {
  move |_, cx| {
    let value = get(&Config::default());
    save(cx, |c| set(c, value))
  }
}

pub(super) fn switch(get: impl Get<bool>, set: impl Set<bool>) -> SettingField<bool> {
  let default = get(&Config::default());
  SettingField::switch(
    move |cx| get(cx.config()),
    move |v, cx| save(cx, |c| set(c, v)),
  )
  .default_value(default)
}

/// The serde name of a config enum value, like `top_right`
fn name<T: Serialize>(value: &T) -> SharedString {
  serde_json::to_value(value)
    .ok()
    .and_then(|v| v.as_str().map(SharedString::from))
    .unwrap_or_default()
}

/// The config enum value named `v`, `None` of an optional one for ""
fn parse<T: DeserializeOwned>(v: &str) -> Option<T> {
  let json = match v.is_empty() {
    true => serde_json::Value::Null,
    false => serde_json::Value::String(v.to_string()),
  };
  serde_json::from_value(json).ok()
}

/// One of a config enum's values, labelled. `None` of an optional one is ""
pub(super) fn choice<T>(
  options: &[(T, Cow<'static, str>)],
  get: impl Get<T>,
  set: impl Set<T>,
) -> SettingField<SharedString>
where
  T: Serialize + DeserializeOwned + 'static,
{
  let default = name(&get(&Config::default()));
  let options: Vec<_> = options
    .iter()
    .map(|(value, label)| (name(value), label.clone().into()))
    .collect();
  SettingField::dropdown(
    options,
    move |cx| name(&get(cx.config())),
    move |v, cx| {
      if let Some(value) = parse::<T>(&v) {
        save(cx, |c| set(c, value));
      }
    },
  )
  .default_value(default)
}

/// An input and the value it last showed or saved, to tell what the user
/// changed from what changed elsewhere
struct Field<S, T> {
  state: Entity<S>,
  last: T,
  _subscription: Subscription,
}

/// Free text, committed on enter or blur. `key` must be unique in the window.
pub(super) fn text(
  key: impl Display,
  get: impl Get<String>,
  set: impl Set<String>,
) -> SettingField<SharedString> {
  let read = {
    let get = get.clone();
    move |cx: &App| get(cx.config())
  };
  let commit = {
    let set = set.clone();
    move |value: String, cx: &mut App| save(cx, |c| set(c, value))
  };
  text_field(format!("field-{key}"), read, commit)
    .on_reset(is_default(get.clone()), reset(get, set))
}

/// An optional setting as text, showing what applies when it is unset
/// (`fallback`). Leaving that or nothing in the field unsets it.
pub(super) fn optional_text(
  key: impl Display,
  get: impl Get<Option<String>>,
  set: impl Set<Option<String>>,
  fallback: impl Fn(&App) -> String + Clone + 'static,
) -> SettingField<SharedString> {
  let read = {
    let (get, fallback) = (get.clone(), fallback.clone());
    move |cx: &App| get(cx.config()).unwrap_or_else(|| fallback(cx))
  };
  let commit = {
    let (get, set) = (get.clone(), set.clone());
    move |value: String, cx: &mut App| {
      let value = optional(value, &fallback(cx));
      if value != get(cx.config()) {
        save(cx, |c| set(c, value));
      }
    }
  };
  text_field(format!("field-{key}"), read, commit).on_reset(
    move |cx: &App| get(cx.config()).is_some(),
    move |_, cx: &mut App| save(cx, |c| set(c, None)),
  )
}

/// What an optional text field holds: none when it is blank or shows the fallback
fn optional(value: String, fallback: &str) -> Option<String> {
  (!value.trim().is_empty() && value != fallback).then_some(value)
}

fn text_field(
  id: String,
  read: impl Fn(&App) -> String + Clone + 'static,
  commit: impl Fn(String, &mut App) + Clone + 'static,
) -> SettingField<SharedString> {
  SettingField::render(
    move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
      let (read, commit) = (read.clone(), commit.clone());
      let current = read(cx);
      let field = window.use_keyed_state(id.clone(), cx, |window, cx| {
        let shown = read(cx);
        let input = cx.new(|cx| InputState::new(window, cx).default_value(shown.clone()));
        let _subscription = cx.subscribe(
          &input,
          move |field: &mut Field<InputState, String>, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
              let value = input.read(cx).value().to_string();
              if value != field.last {
                field.last = value.clone();
                commit(value, cx);
              }
            }
          },
        );
        Field {
          state: input,
          last: shown,
          _subscription,
        }
      });

      // follow changes made elsewhere
      field.update(cx, |field, cx| {
        if current != field.last {
          field.last = current.clone();
          field
            .state
            .update(cx, |input, cx| input.set_value(current, window, cx));
        }
      });

      let input = field.read(cx).state.clone();
      Input::new(&input)
        .with_size(options.size())
        .disabled(options.is_disabled())
        .map(|i| match options.layout().is_horizontal() {
          true => i.w_64(),
          false => i.w_full(),
        })
    },
  )
}

/// A number, committed on enter, blur or a step. `key` must be unique in the window.
pub(super) fn number(
  key: impl Display,
  (min, max, step): (f64, f64, f64),
  get: impl Get<f64>,
  set: impl Set<f64>,
) -> SettingField<SharedString> {
  let id = format!("field-{key}");
  let (get_, set_) = (get.clone(), set.clone());
  SettingField::render(
    move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
      let (get, set) = (get_.clone(), set_.clone());
      let current = get(cx.config());
      let field = window.use_keyed_state(id.clone(), cx, |window, cx| {
        let input = cx.new(|cx| {
          InputState::new(window, cx)
            .default_value(format_number(get(cx.config())))
            .step(step)
            .min(min)
            .max(max)
        });
        let _subscription = cx.subscribe_in(
          &input,
          window,
          move |field: &mut Field<InputState, f64>, input, event: &InputEvent, window, cx| {
            let typing = input.read(cx).focus_handle(cx).is_focused(window);
            // a keystroke may be half a number; a step (buttons, arrows) or leaving is whole
            let commit = match event {
              InputEvent::PressEnter { .. } | InputEvent::Blur => true,
              InputEvent::Change => !typing,
              InputEvent::Focus => false,
            };
            let Ok(value) = input.read(cx).value().trim().parse::<f64>() else {
              return;
            };
            let value = value.clamp(min, max);
            if commit && value != field.last {
              field.last = value;
              save(cx, |c| set(c, value));
            }
          },
        );
        Field {
          state: input,
          last: get(cx.config()),
          _subscription,
        }
      });

      field.update(cx, |field, cx| {
        if current != field.last {
          field.last = current;
          field.state.update(cx, |input, cx| {
            input.set_value(format_number(current), window, cx)
          });
        }
      });

      let input = field.read(cx).state.clone();
      NumberInput::new(&input)
        .with_size(options.size())
        .disabled(options.is_disabled())
        .map(|i| match options.layout().is_horizontal() {
          true => i.w(px(128.)),
          false => i.w_full(),
        })
    },
  )
  .on_reset(is_default(get.clone()), reset(get, set))
}

fn format_number(value: f64) -> String {
  // 1.0 reads better as 1, and 0.30000000000000004 as 0.3
  ((value * 1000.).round() / 1000.).to_string()
}

/// A value on a slider, committed when it is let go. `key` must be unique in the window.
pub(super) fn slider(
  key: impl Display,
  (min, max, step): (f32, f32, f32),
  get: impl Get<f32>,
  set: impl Set<f32>,
) -> SettingField<SharedString> {
  let id = format!("field-{key}");
  let (get_, set_) = (get.clone(), set.clone());
  SettingField::render(
    move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
      let (get, set) = (get_.clone(), set_.clone());
      let current = get(cx.config());
      let field = window.use_keyed_state(id.clone(), cx, |_, cx| {
        let slider = cx.new(|_| {
          SliderState::new()
            .min(min)
            .max(max)
            .step(step)
            .default_value(current)
        });
        let _subscription = cx.subscribe(
          &slider,
          move |field: &mut Field<SliderState, f32>, _, event: &SliderEvent, cx| match event {
            SliderEvent::Change(_) => cx.notify(),
            SliderEvent::Release(value) => {
              let value = value.start();
              if value != field.last {
                field.last = value;
                save(cx, |c| set(c, value));
              }
            }
          },
        );
        Field {
          state: slider,
          last: current,
          _subscription,
        }
      });

      // follow changes made elsewhere; while dragging nothing else changes it
      field.update(cx, |field, cx| {
        if current != field.last {
          field.last = current;
          field
            .state
            .update(cx, |s, cx| s.set_value(current, window, cx));
        }
      });

      let slider = field.read(cx).state.clone();
      let shown = slider.read(cx).value().start();
      div()
        .flex()
        .items_center()
        .gap_2()
        .map(|d| match options.layout().is_horizontal() {
          true => d.w(px(256.)),
          false => d.w_full(),
        })
        .child(
          div()
            .w(px(40.))
            .text_right()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .child(format_number(f64::from(shown))),
        )
        .child(
          div()
            .flex_1()
            .child(Slider::new(&slider).disabled(options.is_disabled())),
        )
    },
  )
  .on_reset(is_default(get.clone()), reset(get, set))
}

/// An entry of a [`searchable`] select
#[derive(Clone)]
pub(super) struct Choice {
  pub value: String,
  pub label: String,
}

impl SearchableListItem for Choice {
  type Value = String;

  fn title(&self) -> SharedString {
    self.label.clone().into()
  }

  fn value(&self) -> &String {
    &self.value
  }
}

type Choices = SearchableVec<Choice>;

struct Searchable {
  state: Entity<SelectState<Choices>>,
  _subscription: Subscription,
}

/// A select with a search box, for long lists. It shows `selected`, or the
/// placeholder when that is `None` (so a pick that changes nothing, like adding
/// a widget, goes back to it). `key` must be unique in the window.
pub(super) fn searchable(
  key: impl Display,
  choices: Vec<Choice>,
  selected: Option<String>,
  placeholder: impl Into<SharedString>,
  window: &mut Window,
  cx: &mut App,
  on_pick: impl Fn(String, &mut App) + 'static,
) -> Select<Choices> {
  let field = window.use_keyed_state(format!("select-{key}"), cx, |window, cx| {
    let state = cx.new(|cx| {
      SelectState::new(SearchableVec::new(choices.clone()), None, window, cx).searchable(true)
    });
    let _subscription = cx.subscribe(&state, move |_, _, event: &SelectEvent<Choices>, cx| {
      let SelectEvent::Confirm(Some(value)) = event else {
        return;
      };
      on_pick(value.clone(), cx);
    });
    Searchable {
      state,
      _subscription,
    }
  });

  let state = field.read(cx).state.clone();
  if state.read(cx).selected_value() != selected.as_ref() {
    state.update(cx, |state, cx| match &selected {
      Some(value) => state.set_selected_value(value, window, cx),
      None => state.set_selected_index(None, window, cx),
    });
  }
  Select::new(&state)
    .placeholder(placeholder)
    .search_placeholder(t!("app.settings.search"))
}

#[cfg(test)]
mod tests {
  use corona_config::{NotificationPosition, Weekday};

  use super::{format_number, name, optional, parse};

  #[test]
  fn names_and_numbers() {
    assert_eq!(name(&NotificationPosition::TopRight).as_ref(), "top_right");
    assert_eq!(name(&Weekday::Sunday).as_ref(), "sunday");
    assert_eq!(format_number(1.0), "1");
    assert_eq!(format_number(0.1 + 0.2), "0.3");
  }

  #[test]
  fn optional_names_round_trip() {
    assert_eq!(name(&None::<NotificationPosition>).as_ref(), "");
    assert_eq!(parse::<Option<NotificationPosition>>(""), Some(None));
    for value in [
      NotificationPosition::TopRight,
      NotificationPosition::TopLeft,
    ] {
      assert_eq!(parse(&name(&value)), Some(value));
      assert_eq!(parse(&name(&Some(value))), Some(Some(value)));
    }
  }

  #[test]
  fn parse_rejects_unknown() {
    assert_eq!(parse::<NotificationPosition>("nowhere"), None);
    // a required enum has no empty value
    assert_eq!(parse::<NotificationPosition>(""), None);
    assert_eq!(parse::<Weekday>("Sunday"), None);
  }

  #[test]
  fn format_number_rounds_to_thousandths() {
    assert_eq!(format_number(0.), "0");
    assert_eq!(format_number(-2.5), "-2.5");
    assert_eq!(format_number(1.23456), "1.235");
    assert_eq!(format_number(100.), "100");
  }

  #[test]
  fn optional_text_values() {
    assert_eq!(optional("".into(), "fb"), None);
    assert_eq!(optional("  ".into(), "fb"), None);
    assert_eq!(optional("fb".into(), "fb"), None);
    assert_eq!(optional("x".into(), "fb"), Some("x".into()));
    // kept as typed
    assert_eq!(optional(" x ".into(), "fb"), Some(" x ".into()));
    assert_eq!(optional("x".into(), ""), Some("x".into()));
  }
}
