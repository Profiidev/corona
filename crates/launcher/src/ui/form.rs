use gpui_kit::{
  App, AppContext, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
  Render, SharedString, Styled, Subscription, Window,
  component::{
    ActiveTheme, Sizable,
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    scroll::ScrollableElement,
    searchable_list::{SearchableListItem, SearchableVec},
    select::{Select, SelectState},
  },
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

use crate::view::{Cancel, Field, FieldKind, FormValues, FormView, Submit};

#[derive(Clone)]
struct Choice {
  value: String,
  label: SharedString,
}

impl SearchableListItem for Choice {
  type Value = String;

  fn title(&self) -> SharedString {
    self.label.clone()
  }

  fn value(&self) -> &String {
    &self.value
  }
}

enum Control {
  Input(Entity<InputState>),
  Area(Entity<TextareaState>),
  Dropdown(Entity<SelectState<SearchableVec<Choice>>>),
  Checkbox(bool),
}

struct FieldState {
  field: Field,
  control: Control,
  initial: String,
  error: Option<SharedString>,
}

pub struct FormPage {
  pub cancel: Option<Cancel>,
  message: Option<SharedString>,
  submit_label: SharedString,
  submit: Submit,
  fields: Vec<FieldState>,
  _subscriptions: Vec<Subscription>,
}

impl FormPage {
  pub fn new(form: FormView, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let mut subscriptions = Vec::new();
    let fields = form
      .fields
      .into_iter()
      .enumerate()
      .map(|(index, field)| {
        let control = match &field.kind {
          FieldKind::Text | FieldKind::Password => {
            let masked = field.kind == FieldKind::Password;
            let (value, placeholder) = (field.value.clone(), field.placeholder.clone());
            let state = cx.new(|cx| {
              InputState::new(window, cx)
                .masked(masked)
                .placeholder(placeholder)
                .default_value(value)
            });
            // validate on blur
            subscriptions.push(cx.subscribe(&state, move |this, _, e: &InputEvent, cx| {
              if let InputEvent::Blur = e {
                this.check(index, cx);
              }
            }));
            Control::Input(state)
          }
          FieldKind::TextArea => {
            let value = field.value.clone();
            Control::Area(cx.new(|cx| {
              TextareaState::new(window, cx)
                .auto_grow(3, 8)
                .default_value(value)
            }))
          }
          FieldKind::Dropdown(options) => {
            let choices: Vec<Choice> = options
              .iter()
              .map(|(value, label)| Choice {
                value: value.to_string(),
                label: label.clone(),
              })
              .collect();
            let selected = options.iter().position(|(v, _)| *v == field.value);
            Control::Dropdown(cx.new(|cx| {
              SelectState::new(
                SearchableVec::new(choices),
                selected.map(gpui_kit::base::IndexPath::new),
                window,
                cx,
              )
            }))
          }
          FieldKind::Checkbox => Control::Checkbox(field.value == "true"),
        };
        FieldState {
          initial: field.value.clone(),
          field,
          control,
          error: None,
        }
      })
      .collect();
    Self {
      cancel: form.cancel,
      message: form.message,
      submit_label: form.submit_label,
      submit: form.submit,
      fields,
      _subscriptions: subscriptions,
    }
  }

  pub fn submit_label(&self) -> &SharedString {
    &self.submit_label
  }

  fn value(&self, state: &FieldState, cx: &App) -> String {
    match &state.control {
      Control::Input(input) => input.read(cx).value().to_string(),
      Control::Area(area) => area.read(cx).value().to_string(),
      Control::Dropdown(select) => select
        .read(cx)
        .selected_value()
        .cloned()
        .unwrap_or_default(),
      Control::Checkbox(checked) => checked.to_string(),
    }
  }

  fn focus_handle(state: &FieldState, cx: &App) -> Option<gpui_kit::FocusHandle> {
    match &state.control {
      Control::Input(input) => Some(input.focus_handle(cx)),
      Control::Area(area) => Some(area.focus_handle(cx)),
      Control::Dropdown(select) => Some(select.focus_handle(cx)),
      Control::Checkbox(_) => None,
    }
  }

  pub fn focus_first(&self, window: &mut Window, cx: &mut App) {
    if let Some(handle) = self.fields.iter().find_map(|f| Self::focus_handle(f, cx)) {
      window.focus(&handle, cx);
    }
  }

  /// Edited since it opened
  pub fn dirty(&self, cx: &App) -> bool {
    self.fields.iter().any(|f| self.value(f, cx) != f.initial)
  }

  fn check(&mut self, index: usize, cx: &mut Context<Self>) {
    let Some(state) = self.fields.get(index) else {
      return;
    };
    let value = self.value(state, cx);
    let error = state
      .field
      .error(&value, t!("app.launcher.required").into());
    self.fields[index].error = error;
    cx.notify();
  }

  /// Enter moves to the next field and submits on the last one; a text area keeps it
  pub fn on_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
    let focused = self
      .fields
      .iter()
      .position(|f| Self::focus_handle(f, cx).is_some_and(|h| h.is_focused(window)));
    let Some(index) = focused else {
      return true;
    };
    if matches!(self.fields[index].control, Control::Area(_)) {
      cx.propagate();
      return false;
    }
    self.check(index, cx);
    let next = self.fields[index + 1..]
      .iter()
      .find_map(|f| Self::focus_handle(f, cx));
    match next {
      Some(handle) => {
        window.focus(&handle, cx);
        false
      }
      None => true,
    }
  }

  /// The values when every field is valid; otherwise marks the errors and focuses the first
  pub fn validate(
    &mut self,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Option<(FormValues, Submit)> {
    for index in 0..self.fields.len() {
      self.check(index, cx);
    }
    if let Some(bad) = self.fields.iter().find(|f| f.error.is_some()) {
      if let Some(handle) = Self::focus_handle(bad, cx) {
        window.focus(&handle, cx);
      }
      return None;
    }
    let values = self
      .fields
      .iter()
      .map(|f| (f.field.id.clone(), self.value(f, cx)))
      .collect();
    Some((values, self.submit.clone()))
  }
}

impl Render for FormPage {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let muted = theme.colors.muted_foreground;
    let danger = theme.colors.danger;
    let rows = self.fields.iter().enumerate().map(|(index, state)| {
      let control = match &state.control {
        Control::Input(input) => {
          let masked = state.field.kind == FieldKind::Password;
          Input::new(input)
            .when(masked, |i| i.mask_toggle())
            .into_any_element()
        }
        Control::Area(area) => Textarea::new(area).into_any_element(),
        Control::Dropdown(select) => Select::new(select).small().into_any_element(),
        Control::Checkbox(checked) => Checkbox::new(("form-check", index))
          .checked(*checked)
          .on_click(cx.listener(move |this, checked: &bool, _, cx| {
            this.fields[index].control = Control::Checkbox(*checked);
            cx.notify();
          }))
          .into_any_element(),
      };
      div()
        .flex()
        .gap_4()
        .items_start()
        .child(
          div()
            .w(px(140.))
            .pt(px(6.))
            .text_size(px(13.))
            .text_color(muted)
            .text_right()
            .child(state.field.label.clone()),
        )
        .child(
          div()
            .flex_1()
            .flex()
            .flex_col()
            .gap_1()
            .child(control)
            .when_some(state.error.clone(), |el, error| {
              el.child(div().text_size(px(12.)).text_color(danger).child(error))
            }),
        )
    });
    div()
      .id("launcher-form")
      .size_full()
      .overflow_y_scrollbar()
      .p(px(20.))
      .flex()
      .flex_col()
      .gap_3()
      .when_some(self.message.clone(), |el, message| {
        el.child(div().text_size(px(13.)).text_color(muted).child(message))
      })
      .children(rows)
      .child(
        div()
          .pt_2()
          .text_size(px(12.))
          .text_color(muted)
          .child(t!("app.launcher.form_hint", action = self.submit_label.clone()).to_string()),
      )
  }
}
