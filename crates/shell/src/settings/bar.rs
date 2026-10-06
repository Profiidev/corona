//! The bar page: each bar's layout, and its widgets, dragged between the
//! sections and into groups, with a form for each widget's options.

use corona_config::{
  Config, ConfigProvider,
  bar::{BarConfig, WidgetConfig, WidgetEntry},
  placement::Placement,
};
use std::{cell::Cell, rc::Rc};

use corona_surface::bar::BarState;
use gpui_kit::base::Disableable;
use gpui_kit::{
  Anchor, App, AppContext, Div, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
  SharedString, StatefulInteractiveElement, Styled, Window,
  assets::IconName,
  base::ElementExt,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    popover::Popover,
    setting::{SettingGroup, SettingItem, SettingPage},
  },
  div, px,
};

use crate::{
  settings::fields::{Choice, choice, number, save, searchable, slider, switch},
  widgets::{privacy, resource, tray},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
  Start,
  Center,
  End,
}

impl Section {
  const ALL: [Section; 3] = [Section::Start, Section::Center, Section::End];

  fn label(self) -> &'static str {
    match self {
      Section::Start => "Start",
      Section::Center => "Center",
      Section::End => "End",
    }
  }

  fn of(self, bar: &BarConfig) -> &Vec<WidgetConfig> {
    match self {
      Section::Start => &bar.start,
      Section::Center => &bar.center,
      Section::End => &bar.end,
    }
  }

  fn of_mut(self, bar: &mut BarConfig) -> &mut Vec<WidgetConfig> {
    match self {
      Section::Start => &mut bar.start,
      Section::Center => &mut bar.center,
      Section::End => &mut bar.end,
    }
  }
}

/// Where a widget sits: an entry of a section, or a member of a group entry
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
  pub section: Section,
  pub index: usize,
  pub member: Option<usize>,
}

/// Where a dragged widget goes
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Target {
  /// In front of the widget at this slot
  Before(Slot),
  /// At the end of a section
  End(Section),
  /// Into the group at this entry, or a new group with the widget there
  Join { section: Section, index: usize },
}

/// Marks the dragged entry while it is moved, removed once it landed
const MOVING: &str = "\0moving";

fn placeholder() -> WidgetEntry {
  WidgetEntry {
    widget_type: MOVING.to_string(),
    options: None,
  }
}

/// Moves the widget (or group) at `from` to `to`. A group left with one widget
/// becomes that widget, an empty one goes.
pub(crate) fn move_widget(bar: &mut BarConfig, from: Slot, to: Target) {
  // swap a placeholder in, so indices of `to` stay valid while inserting
  let entries = from.section.of_mut(bar);
  let Some(entry) = entries.get_mut(from.index) else {
    return;
  };
  let moving: WidgetConfig = match (entry, from.member) {
    (WidgetConfig::Group { group }, Some(m)) if m < group.len() => {
      WidgetConfig::Widget(std::mem::replace(&mut group[m], placeholder()))
    }
    (entry, None) => std::mem::replace(entry, WidgetConfig::Widget(placeholder())),
    _ => return,
  };

  match to {
    Target::Before(slot) => {
      let entries = slot.section.of_mut(bar);
      match (slot.member, &moving, entries.get_mut(slot.index)) {
        (Some(m), WidgetConfig::Widget(widget), Some(WidgetConfig::Group { group })) => {
          group.insert(m.min(group.len()), widget.clone());
        }
        _ => entries.insert(slot.index.min(entries.len()), moving),
      }
    }
    Target::End(section) => section.of_mut(bar).push(moving),
    Target::Join { section, index } => {
      let entries = section.of_mut(bar);
      let members = match moving {
        WidgetConfig::Widget(widget) => vec![widget],
        WidgetConfig::Group { group } => group,
      };
      match entries.get_mut(index) {
        Some(WidgetConfig::Group { group }) => group.extend(members),
        Some(entry @ WidgetConfig::Widget(_)) => {
          let WidgetConfig::Widget(there) = entry.clone() else {
            unreachable!()
          };
          *entry = WidgetConfig::Group {
            group: std::iter::once(there).chain(members).collect(),
          };
        }
        None => entries.extend(members.into_iter().map(WidgetConfig::Widget)),
      }
    }
  }

  for section in Section::ALL {
    tidy(section.of_mut(bar));
  }
}

/// Drops the placeholder, and groups too small to be one
fn tidy(entries: &mut Vec<WidgetConfig>) {
  entries.retain_mut(|entry| match entry {
    WidgetConfig::Widget(widget) => widget.widget_type != MOVING,
    WidgetConfig::Group { group } => {
      group.retain(|w| w.widget_type != MOVING);
      if group.len() == 1 {
        *entry = WidgetConfig::Widget(group.remove(0));
      }
      !matches!(entry, WidgetConfig::Group { group } if group.is_empty())
    }
  });
}

fn remove_widget(bar: &mut BarConfig, from: Slot) {
  let entries = from.section.of_mut(bar);
  match (from.member, entries.get_mut(from.index)) {
    (Some(m), Some(WidgetConfig::Group { group })) if m < group.len() => {
      group.remove(m);
    }
    (None, Some(_)) => {
      entries.remove(from.index);
    }
    _ => {}
  }
  tidy(entries);
}

fn widget_at(bar: &BarConfig, slot: Slot) -> Option<&WidgetEntry> {
  match (slot.member, slot.section.of(bar).get(slot.index)?) {
    (None, WidgetConfig::Widget(widget)) => Some(widget),
    (Some(m), WidgetConfig::Group { group }) => group.get(m),
    _ => None,
  }
}

fn widget_at_mut(bar: &mut BarConfig, slot: Slot) -> Option<&mut WidgetEntry> {
  match (slot.member, slot.section.of_mut(bar).get_mut(slot.index)?) {
    (None, WidgetConfig::Widget(widget)) => Some(widget),
    (Some(m), WidgetConfig::Group { group }) => group.get_mut(m),
    _ => None,
  }
}

/// Changes the bar called `name`, if it still exists
fn edit_bar(cx: &mut App, name: &str, edit: impl FnOnce(&mut BarConfig)) {
  let name = name.to_string();
  save(cx, move |c| {
    if let Some(bar) = c.bar.get_mut(&name) {
      edit(bar);
    }
  });
}

pub(super) fn page(cx: &App) -> SettingPage {
  let names: Vec<String> = cx.config().bar.keys().cloned().collect();
  let mut groups: Vec<SettingGroup> = names.iter().map(|name| bar_group(name.clone())).collect();
  groups.push(
    SettingGroup::new()
      .title("Bars")
      .item(SettingItem::render(|_, window, cx| add_bar(window, cx))),
  );
  SettingPage::new("Bar")
    .icon(Icon::new(IconName::PanelTop))
    .resettable(true)
    .groups(groups)
}

/// A bar setting: `field` of the bar called `name`; a bar not in the defaults
/// defaults to a plain bar's values
fn of<T: 'static>(
  name: &str,
  field: fn(&BarConfig) -> T,
) -> impl Fn(&Config) -> T + Clone + 'static {
  let name = name.to_string();
  move |c: &Config| match c.bar.get(&name) {
    Some(bar) => field(bar),
    None => field(&BarConfig::default()),
  }
}

fn set<T: 'static>(
  name: &str,
  field: fn(&mut BarConfig, T),
) -> impl Fn(&mut Config, T) + Clone + 'static {
  let name = name.to_string();
  move |c: &mut Config, value: T| {
    if let Some(bar) = c.bar.get_mut(&name) {
      field(bar, value);
    }
  }
}

fn bar_group(name: String) -> SettingGroup {
  let key = |field: &str| format!("bar.{name}.{field}");
  let editor_name = name.clone();
  let remove_name = name.clone();
  SettingGroup::new()
    .title(SharedString::from(format!("Bar \u{201c}{name}\u{201d}")))
    .items(vec![
      SettingItem::new(
        "Position",
        choice(
          &[
            (Placement::Top, "Top"),
            (Placement::Bottom, "Bottom"),
            (Placement::Left, "Left"),
            (Placement::Right, "Right"),
          ],
          of(&name, |b| b.position),
          set(&name, |b, v| b.position = v),
        ),
      )
      .description("Which screen edge the bar sits at."),
      SettingItem::new(
        "Thickness",
        number(
          key("thickness"),
          (16., 96., 1.),
          of(&name, |b| b.thickness.into()),
          set(&name, |b, v: f64| b.thickness = v as f32),
        ),
      )
      .description("Height of a horizontal bar, width of a vertical one."),
      SettingItem::new(
        "Background opacity",
        slider(
          key("opacity"),
          (0., 1., 0.05),
          of(&name, |b| b.background_opacity),
          set(&name, |b, v| b.background_opacity = v),
        ),
      )
      .description("Under 1 lets the desktop show through."),
      SettingItem::new(
        "Capsules",
        switch(of(&name, |b| b.capsule), set(&name, |b, v| b.capsule = v)),
      )
      .description("A pill behind each widget."),
      SettingItem::new(
        "Widget spacing",
        number(
          key("spacing"),
          (0., 64., 1.),
          of(&name, |b| b.widget_spacing.into()),
          set(&name, |b, v: f64| b.widget_spacing = v as f32),
        ),
      )
      .description("Space between widgets."),
      SettingItem::new(
        "Padding at the start",
        number(
          key("padding_start"),
          (0., 400., 1.),
          of(&name, |b| b.padding_start.into()),
          set(&name, |b, v: f64| b.padding_start = v as f32),
        ),
      )
      .description("Space before the first widget."),
      SettingItem::new(
        "Padding at the end",
        number(
          key("padding_end"),
          (0., 400., 1.),
          of(&name, |b| b.padding_end.into()),
          set(&name, |b, v: f64| b.padding_end = v as f32),
        ),
      )
      .description("Space after the last widget."),
      SettingItem::render(move |_, window, cx| editor(&editor_name, window, cx))
        .keywords(["widgets", "widget"]),
      SettingItem::render(move |_, _, cx| remove_bar(&remove_name, cx)),
    ])
}

fn add_bar(window: &mut Window, cx: &mut App) -> Div {
  let new_name = text_input("bar.new", window, cx);
  div()
    .flex()
    .items_center()
    .gap_2()
    .child(
      div()
        .flex_1()
        .child(gpui_kit::component::input::Input::new(&new_name).small()),
    )
    .child(
      Button::new("bar-add")
        .label("Add bar")
        .cursor_pointer()
        .small()
        .on_click(move |_, window, cx| {
          let name = new_name.read(cx).value().trim().to_string();
          if name.is_empty() || cx.config().bar.contains_key(&name) {
            return;
          }
          save(cx, |c| {
            c.bar.insert(name, BarConfig::default());
          });
          new_name.update(cx, |input, cx| input.set_value("", window, cx));
        }),
    )
}

fn remove_bar(name: &str, cx: &App) -> Div {
  let only = cx.config().bar.len() <= 1;
  let name = name.to_string();
  div().flex().justify_end().child(
    Button::new(SharedString::from(format!("bar-remove-{name}")))
      .label("Remove this bar")
      .cursor_pointer()
      .small()
      .danger()
      .disabled(only)
      .on_click(move |_, _, cx| {
        let name = name.clone();
        save(cx, move |c| {
          c.bar.remove(&name);
        })
      }),
  )
}

/// A free text input that lives as long as the window
fn text_input(
  key: &str,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::Entity<gpui_kit::component::input::InputState> {
  window.use_keyed_state(
    SharedString::from(format!("input-{key}")),
    cx,
    gpui_kit::component::input::InputState::new,
  )
}

#[derive(Clone)]
struct WidgetDrag {
  bar: String,
  from: Slot,
  label: SharedString,
  has_options: bool,
  /// the dragged row's width, for a preview that looks the same
  width: Rc<Cell<Pixels>>,
}

/// What follows the cursor: the row being dragged, as it looks in its column
struct DragPreview(WidgetDrag);

impl Render for DragPreview {
  fn render(&mut self, _: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
    let drag = &self.0;
    let (options, remove) = row_buttons(|what| format!("preview-{what}").into(), drag.has_options);
    row_body(drag.label.clone(), options, remove, cx)
      .w(drag.width.get())
      .opacity(0.9)
  }
}

fn editor(name: &str, window: &mut Window, cx: &mut App) -> Div {
  let Some(bar) = cx.config().bar.get(name).cloned() else {
    return div();
  };
  let mut columns = div().flex().gap_2().w_full();
  for section in Section::ALL {
    columns = columns.child(column(name, &bar, section, window, cx));
  }
  div()
    .flex()
    .flex_col()
    .gap_2()
    .w_full()
    .child(
      div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child("Drag a widget between others to move it, onto one to group them."),
    )
    .child(columns)
}

/// A strip between rows: a widget dropped there lands at `target`
fn gap(id: SharedString, name: &str, target: Target, cx: &App) -> impl IntoElement {
  let line = cx.theme().colors.primary;
  let name = name.to_string();
  div()
    .id(id)
    .h(px(8.))
    .rounded_full()
    .drag_over::<WidgetDrag>(move |style, _, _, _| style.bg(line.opacity(0.5)))
    .on_drop(move |drag: &WidgetDrag, _, cx| {
      if drag.bar == name {
        let from = drag.from;
        edit_bar(cx, &name, |b| move_widget(b, from, target));
      }
    })
}

fn column(name: &str, bar: &BarConfig, section: Section, window: &mut Window, cx: &mut App) -> Div {
  let theme = cx.theme();
  let (radius, accent, border, muted) = (
    theme.radius,
    theme.colors.accent,
    theme.border,
    theme.muted_foreground,
  );
  let hover = theme.tokens.button_hover;
  let id = |what: String| SharedString::from(format!("{what}-{name}-{section:?}"));

  let mut rows: Vec<gpui_kit::AnyElement> = Vec::new();
  for (index, entry) in section.of(bar).iter().enumerate() {
    let at = Slot {
      section,
      index,
      member: None,
    };
    rows.push(gap(id(format!("gap-{index}")), name, Target::Before(at), cx).into_any_element());
    match entry {
      WidgetConfig::Widget(widget) => {
        rows.push(row(name, bar, at, widget, window, cx).into_any_element())
      }
      WidgetConfig::Group { group } => {
        let join_name = name.to_string();
        let mut members: Vec<gpui_kit::AnyElement> = Vec::new();
        for (member, widget) in group.iter().enumerate() {
          let slot = Slot {
            section,
            index,
            member: Some(member),
          };
          if member > 0 {
            members.push(
              gap(
                id(format!("gap-{index}-{member}")),
                name,
                Target::Before(slot),
                cx,
              )
              .into_any_element(),
            );
          }
          members.push(row(name, bar, slot, widget, window, cx).into_any_element());
        }
        rows.push(
          div()
            .id(id(format!("group-{index}")))
            .flex()
            .flex_col()
            .p_1()
            .rounded(radius)
            .border_1()
            .border_dashed()
            .border_color(border)
            // the group's own frame takes drops too: into the group
            .drag_over::<WidgetDrag>(move |style, _, _, _| style.bg(hover))
            .on_drop(move |drag: &WidgetDrag, _, cx| {
              if drag.bar == join_name {
                let from = drag.from;
                edit_bar(cx, &join_name, |b| {
                  move_widget(b, from, Target::Join { section, index })
                });
              }
            })
            .child(div().pb_1().text_xs().text_color(muted).child("Group"))
            .children(members)
            .into_any_element(),
        );
      }
    }
  }

  let drop_name = name.to_string();
  div()
    .flex_1()
    .min_w_0()
    .flex()
    .flex_col()
    .p_2()
    .rounded(radius)
    .bg(accent)
    .child(
      div()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .mb_1()
        .child(
          div()
            .text_sm()
            .font_weight(gpui_kit::FontWeight::BOLD)
            .child(section.label()),
        )
        .child(div().w_48().child(add_widget(name, section, window, cx))),
    )
    .children(rows)
    // the rest of the column takes drops at its end
    .child(
      div()
        .id(id("end".into()))
        .min_h(px(32.))
        .flex_1()
        .rounded(radius)
        .drag_over::<WidgetDrag>(move |style, _, _, _| style.bg(hover))
        .on_drop(move |drag: &WidgetDrag, _, cx| {
          if drag.bar == drop_name {
            let from = drag.from;
            edit_bar(cx, &drop_name, |b| {
              move_widget(b, from, Target::End(section))
            });
          }
        }),
    )
}

fn add_widget(name: &str, section: Section, window: &mut Window, cx: &mut App) -> impl IntoElement {
  let choices = BarState::widget_names(cx)
    .into_iter()
    .map(|widget| Choice {
      label: title(&widget).into(),
      value: widget.into(),
    })
    .collect();
  let name_ = name.to_string();
  searchable(
    format!("add-{name}-{section:?}"),
    choices,
    None,
    "Add widget",
    window,
    cx,
    move |widget, cx| {
      edit_bar(cx, &name_, |b| {
        section.of_mut(b).push(WidgetConfig::Widget(WidgetEntry {
          widget_type: widget.to_string(),
          options: None,
        }))
      });
    },
  )
  .xsmall()
  .w_full()
  .menu_width(px(220.))
}

/// `active_window` as "Active window"
fn title(widget_type: &str) -> String {
  let mut title = widget_type.replace('_', " ");
  if let Some(first) = title.get_mut(..1) {
    first.make_ascii_uppercase();
  }
  title
}

/// The options and remove buttons of a row
fn row_buttons(id: impl Fn(&str) -> SharedString, has_options: bool) -> (Option<Button>, Button) {
  let options = has_options.then(|| {
    Button::new(id("options"))
      .icon(IconName::Settings2)
      .cursor_pointer()
      .ghost()
      .xsmall()
  });
  let remove = Button::new(id("remove"))
    .icon(IconName::X)
    .cursor_pointer()
    .ghost()
    .xsmall();
  (options, remove)
}

/// How a widget's row looks, shared by the row and its drag preview
fn row_body(
  label: SharedString,
  options: Option<impl IntoElement>,
  remove: Button,
  cx: &App,
) -> Div {
  let theme = cx.theme();
  div()
    .flex()
    .items_center()
    .gap_1()
    .px_1()
    .min_h(px(28.))
    .rounded(theme.radius)
    .bg(theme.colors.background)
    .text_color(theme.foreground)
    .child(
      Icon::new(IconName::GripVertical)
        .xsmall()
        .text_color(theme.muted_foreground),
    )
    .child(div().flex_1().min_w_0().truncate().text_sm().child(label))
    .children(options)
    .child(remove)
}

fn row(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  widget: &WidgetEntry,
  window: &mut Window,
  cx: &mut App,
) -> impl IntoElement {
  let hover = cx.theme().tokens.button_hover;
  let label: SharedString = match widget.widget_type.as_str() {
    // several of these sit side by side, what they show tells them apart
    "resource" => {
      let options: resource::Options = options(bar, slot);
      resource::stat_name(options.stat).into()
    }
    other => title(other).into(),
  };
  let form = options_form(&widget.widget_type);
  let has_options = form.is_some();
  let id = |what: &str| {
    SharedString::from(format!(
      "{what}-{name}-{:?}-{}-{:?}",
      slot.section, slot.index, slot.member
    ))
  };
  let width = window.use_keyed_state(id("width"), cx, |_, _| Rc::new(Cell::new(Pixels::ZERO)));
  let width = width.read(cx).clone();
  let drag = WidgetDrag {
    bar: name.to_string(),
    from: slot,
    label: label.clone(),
    has_options,
    width: width.clone(),
  };
  let (join_name, remove_name, form_name) = (name.to_string(), name.to_string(), name.to_string());

  let (options, remove) = row_buttons(id, has_options);
  let options = options.zip(form).map(|(button, form)| {
    Popover::new(id("options-popover"))
      .anchor(Anchor::TopRight)
      .trigger(button)
      .content(move |_, window, cx| {
        // read fresh: the config changes while the popup is open
        let Some(bar) = cx.config().bar.get(&form_name).cloned() else {
          return div().into_any_element();
        };
        form(&form_name, &bar, slot, window, cx)
      })
  });
  let remove =
    remove.on_click(move |_, _, cx| edit_bar(cx, &remove_name, |b| remove_widget(b, slot)));

  row_body(label, options, remove, cx)
    .id(id("row"))
    .cursor_grab()
    .on_prepaint(move |bounds, _, _| width.set(bounds.size.width))
    .on_drag(drag, |drag: &WidgetDrag, _, _, cx| {
      cx.new(|_| DragPreview(drag.clone()))
    })
    .drag_over::<WidgetDrag>(move |style, _, _, _| style.bg(hover))
    // dropped onto a widget: grouped with it
    .on_drop(move |drag: &WidgetDrag, _, cx| {
      if drag.bar == join_name && drag.from != slot {
        let from = drag.from;
        edit_bar(cx, &join_name, |b| {
          move_widget(
            b,
            from,
            Target::Join {
              section: slot.section,
              index: slot.index,
            },
          )
        });
      }
    })
}

type Form = fn(&str, &BarConfig, Slot, &mut Window, &mut App) -> gpui_kit::AnyElement;

/// The options form of a widget type, if it has options
fn options_form(widget_type: &str) -> Option<Form> {
  match widget_type {
    "clock" => Some(clock_form),
    "resource" => Some(resource_form),
    "tray" => Some(tray_form),
    "privacy" => Some(privacy_form),
    _ => None,
  }
}

/// The widget's options, its defaults filled in
fn options<T: serde::de::DeserializeOwned + serde::Serialize + Default>(
  bar: &BarConfig,
  slot: Slot,
) -> T {
  widget_at(bar, slot)
    .and_then(|w| w.options.clone())
    .and_then(|o| serde_json::from_value(o).ok())
    .unwrap_or_default()
}

/// The widget's options as they are now, for handlers that outlive the render
/// they were made in
fn current_options<T: serde::de::DeserializeOwned + serde::Serialize + Default>(
  cx: &App,
  name: &str,
  slot: Slot,
) -> Option<T> {
  cx.config().bar.get(name).map(|bar| options(bar, slot))
}

/// Writes the widget's options; ones equal to the defaults are left out
fn set_options<T: serde::Serialize + Default>(cx: &mut App, name: &str, slot: Slot, value: T) {
  let json = serde_json::to_value(&value).ok();
  let default = serde_json::to_value(T::default()).ok();
  edit_bar(cx, name, move |b| {
    if let Some(widget) = widget_at_mut(b, slot) {
      widget.options = match (json, default) {
        (Some(serde_json::Value::Object(json)), Some(serde_json::Value::Object(default))) => {
          let changed: serde_json::Map<_, _> = json
            .into_iter()
            .filter(|(k, v)| default.get(k) != Some(v))
            .collect();
          (!changed.is_empty()).then_some(serde_json::Value::Object(changed))
        }
        (json, _) => json,
      };
    }
  });
}

fn form() -> Div {
  div().flex().flex_col().gap_2().w(px(320.))
}

fn form_row(label: &'static str, control: impl IntoElement) -> Div {
  div()
    .flex()
    .items_center()
    .gap_2()
    .child(div().flex_1().text_sm().child(label))
    .child(control)
}

/// A text input in a widget's form, written on enter or blur
fn form_input(
  key: String,
  value: String,
  window: &mut Window,
  cx: &mut App,
  commit: impl Fn(String, &mut App) + 'static,
) -> Div {
  use gpui_kit::component::input::{Input, InputEvent, InputState};
  struct Field {
    input: gpui_kit::Entity<InputState>,
    shown: String,
    _subscription: gpui_kit::Subscription,
  }
  let field = window.use_keyed_state(SharedString::from(key), cx, |window, cx| {
    let input = cx.new(|cx| InputState::new(window, cx).default_value(value.clone()));
    let commit = std::rc::Rc::new(commit);
    let _subscription = cx.subscribe(
      &input,
      move |field: &mut Field, input, event: &InputEvent, cx| {
        if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
          let text = input.read(cx).value().to_string();
          if text != field.shown {
            field.shown = text.clone();
            commit(text, cx);
          }
        }
      },
    );
    Field {
      input,
      shown: value.clone(),
      _subscription,
    }
  });
  field.update(cx, |field, cx| {
    if field.shown != value {
      field.shown = value.clone();
      field
        .input
        .update(cx, |input, cx| input.set_value(value, window, cx));
    }
  });
  let input = field.read(cx).input.clone();
  div().w_48().child(Input::new(&input).small())
}

fn form_key(name: &str, slot: Slot, field: &str) -> String {
  format!(
    "form-{name}-{:?}-{}-{:?}-{field}",
    slot.section, slot.index, slot.member
  )
}

fn clock_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::AnyElement {
  let options: crate::widgets::clock::Options = options(bar, slot);
  let name_ = name.to_string();
  form()
    .child(form_row(
      "Format (strftime)",
      form_input(
        form_key(name, slot, "format"),
        options.format,
        window,
        cx,
        move |format, cx| {
          set_options(cx, &name_, slot, crate::widgets::clock::Options { format });
        },
      ),
    ))
    .into_any_element()
}

fn resource_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::AnyElement {
  let current: resource::Options = options(bar, slot);
  let choices = resource::Stat::ALL
    .iter()
    .map(|stat| Choice {
      value: serde_json::to_value(stat)
        .ok()
        .and_then(|v| v.as_str().map(SharedString::from))
        .unwrap_or_default(),
      label: resource::stat_name(*stat).into(),
    })
    .collect();
  let selected = serde_json::to_value(current.stat)
    .ok()
    .and_then(|v| v.as_str().map(SharedString::from));
  let stat_name = name.to_string();
  let stat = searchable(
    form_key(name, slot, "stat"),
    choices,
    selected,
    "Shows",
    window,
    cx,
    move |value, cx| {
      let Ok(stat) = serde_json::from_value(serde_json::Value::String(value.to_string())) else {
        return;
      };
      // read now: this handler outlives the render it was made in
      let Some(bar) = cx.config().bar.get(&stat_name).cloned() else {
        return;
      };
      let options: resource::Options = options(&bar, slot);
      set_options(cx, &stat_name, slot, resource::Options { stat, ..options });
    },
  )
  .small()
  .w_48();

  // the defaults of the stat shown filled in; one left as it is stays unset
  let defaults = current.stat.thresholds();
  let threshold = |field: &'static str,
                   value: Option<f32>,
                   default: Option<f32>,
                   window: &mut Window,
                   cx: &mut App| {
    let name = name.to_string();
    form_input(
      form_key(&name, slot, field),
      value.or(default).map(|v| v.to_string()).unwrap_or_default(),
      window,
      cx,
      move |text, cx| {
        let Some(mut options) = current_options::<resource::Options>(cx, &name, slot) else {
          return;
        };
        let default = match field {
          "warning" => options.stat.thresholds().map(|t| t.0),
          _ => options.stat.thresholds().map(|t| t.1),
        };
        let value = text
          .trim()
          .parse::<f32>()
          .ok()
          .filter(|v| Some(*v) != default);
        match field {
          "warning" => options.warning = value,
          _ => options.critical = value,
        }
        set_options(cx, &name, slot, options);
      },
    )
  };
  let warning = threshold(
    "warning",
    current.warning,
    defaults.map(|t| t.0),
    window,
    cx,
  );
  let critical = threshold(
    "critical",
    current.critical,
    defaults.map(|t| t.1),
    window,
    cx,
  );
  let mount_name = name.to_string();
  let mount = form_input(
    form_key(name, slot, "mount"),
    current.mount.clone(),
    window,
    cx,
    move |mount, cx| {
      if let Some(options) = current_options::<resource::Options>(cx, &mount_name, slot) {
        set_options(
          cx,
          &mount_name,
          slot,
          resource::Options { mount, ..options },
        );
      }
    },
  );

  form()
    .child(form_row("Shows", stat))
    .child(form_row("Warning at", warning))
    .child(form_row("Critical at", critical))
    .child(form_row("Mount (disk)", mount))
    .into_any_element()
}

fn tray_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::AnyElement {
  let current: tray::Options = options(bar, slot);
  let rows: Vec<_> = current
    .blacklist
    .iter()
    .enumerate()
    .map(|(i, pattern)| {
      let (edit_name, remove_name) = (name.to_string(), name.to_string());
      let remove_list = current.blacklist.clone();
      div()
        .flex()
        .items_center()
        .gap_1()
        .child(form_input(
          form_key(name, slot, &format!("blacklist-{i}")),
          pattern.clone(),
          window,
          cx,
          move |text, cx| {
            let Some(mut options) = current_options::<tray::Options>(cx, &edit_name, slot) else {
              return;
            };
            if let Some(pattern) = options.blacklist.get_mut(i) {
              *pattern = text;
              set_options(cx, &edit_name, slot, options);
            }
          },
        ))
        .child(
          Button::new(SharedString::from(form_key(
            name,
            slot,
            &format!("blacklist-remove-{i}"),
          )))
          .cursor_pointer()
          .icon(IconName::X)
          .ghost()
          .xsmall()
          .on_click(move |_, _, cx| {
            let mut blacklist = remove_list.clone();
            blacklist.remove(i);
            set_options(cx, &remove_name, slot, tray::Options { blacklist });
          }),
        )
    })
    .collect();
  let add_name = name.to_string();
  let add_list = current.blacklist.clone();
  form()
    .child(
      div()
        .text_sm()
        .child("Hidden items: case-insensitive regexes against id and title"),
    )
    .children(rows)
    .child(
      div().child(
        Button::new(SharedString::from(form_key(name, slot, "blacklist-add")))
          .label("Add pattern")
          .cursor_pointer()
          .icon(IconName::Plus)
          .xsmall()
          .outline()
          .on_click(move |_, _, cx| {
            let mut blacklist = add_list.clone();
            blacklist.push(String::new());
            set_options(cx, &add_name, slot, tray::Options { blacklist });
          }),
      ),
    )
    .into_any_element()
}

fn privacy_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  let current: privacy::Options = options(bar, slot);
  let name = name.to_string();
  form()
    .child(form_row(
      "Hide when nothing records",
      gpui_kit::component::switch::Switch::new(SharedString::from(form_key(&name, slot, "idle")))
        .checked(current.hide_when_idle)
        .small()
        .on_click(move |checked, _, cx| {
          set_options(
            cx,
            &name,
            slot,
            privacy::Options {
              hide_when_idle: *checked,
            },
          );
        }),
    ))
    .into_any_element()
}

#[cfg(test)]
mod tests {
  use corona_config::bar::{BarConfig, WidgetConfig, WidgetEntry};

  use super::{Section, Slot, Target, move_widget, remove_widget};

  fn w(t: &str) -> WidgetConfig {
    WidgetConfig::Widget(WidgetEntry {
      widget_type: t.into(),
      options: None,
    })
  }

  fn g(ts: &[&str]) -> WidgetConfig {
    WidgetConfig::Group {
      group: ts
        .iter()
        .map(|t| WidgetEntry {
          widget_type: (*t).into(),
          options: None,
        })
        .collect(),
    }
  }

  fn bar(start: Vec<WidgetConfig>, center: Vec<WidgetConfig>, end: Vec<WidgetConfig>) -> BarConfig {
    BarConfig {
      start,
      center,
      end,
      ..BarConfig::default()
    }
  }

  fn at(section: Section, index: usize) -> Slot {
    Slot {
      section,
      index,
      member: None,
    }
  }

  fn member(section: Section, index: usize, m: usize) -> Slot {
    Slot {
      section,
      index,
      member: Some(m),
    }
  }

  use Section::*;

  #[test]
  fn reorders_within_a_section() {
    let mut b = bar(vec![w("a"), w("b"), w("c")], vec![], vec![]);
    move_widget(&mut b, at(Start, 2), Target::Before(at(Start, 0)));
    assert_eq!(b.start, [w("c"), w("a"), w("b")]);
    move_widget(&mut b, at(Start, 0), Target::End(Start));
    assert_eq!(b.start, [w("a"), w("b"), w("c")]);
  }

  #[test]
  fn moves_across_sections() {
    let mut b = bar(vec![w("a"), w("b")], vec![w("c")], vec![]);
    move_widget(&mut b, at(Start, 0), Target::Before(at(Center, 0)));
    assert_eq!(b.start, [w("b")]);
    assert_eq!(b.center, [w("a"), w("c")]);
    move_widget(&mut b, at(Center, 1), Target::End(End));
    assert_eq!(b.end, [w("c")]);
  }

  #[test]
  fn groups_and_ungroups() {
    let mut b = bar(vec![w("a"), w("b")], vec![], vec![]);
    // onto a widget: a new group
    move_widget(
      &mut b,
      at(Start, 1),
      Target::Join {
        section: Start,
        index: 0,
      },
    );
    assert_eq!(b.start, [g(&["a", "b"])]);
    // out of a group of two: the group dissolves
    move_widget(&mut b, member(Start, 0, 1), Target::End(Center));
    assert_eq!(b.start, [w("a")]);
    assert_eq!(b.center, [w("b")]);
    // into an existing group, and before a member
    let mut b = bar(vec![g(&["a", "b"]), w("c"), w("d")], vec![], vec![]);
    move_widget(
      &mut b,
      at(Start, 1),
      Target::Join {
        section: Start,
        index: 0,
      },
    );
    assert_eq!(b.start, [g(&["a", "b", "c"]), w("d")]);
    move_widget(&mut b, at(Start, 1), Target::Before(member(Start, 0, 0)));
    assert_eq!(b.start, [g(&["d", "a", "b", "c"])]);
  }

  #[test]
  fn removes() {
    let mut b = bar(vec![g(&["a", "b"]), w("c")], vec![], vec![]);
    remove_widget(&mut b, member(Start, 0, 0));
    assert_eq!(b.start, [w("b"), w("c")]);
    remove_widget(&mut b, at(Start, 1));
    assert_eq!(b.start, [w("b")]);
  }
}
