//! Builders for the common command shapes: groups that follow live state, toggles,
//! levels and pick-one lists

// closures are the point of these builders
#![allow(clippy::type_complexity)]

use std::{cell::RefCell, rc::Rc};

use anyhow::{Result, anyhow};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::{
  args::{ArgSpec, ArgValue, Args},
  command::{
    Accessory, Action, Command, CommandSource, Hud, Icon, Meta, Outcome, Section, Tone, simple,
  },
  matching::Query,
  view::ListView,
};

pub type Notify = Rc<dyn Fn(&mut App)>;

/// Calls `notify` whenever `entity` changes
pub fn watch<T: 'static>(
  entity: &gpui_kit::Entity<T>,
  notify: &Notify,
  cx: &mut App,
) -> Subscription {
  let notify = notify.clone();
  cx.observe(entity, move |_, cx| notify(cx))
}
pub type ObserveFn = Rc<dyn Fn(Notify, &mut App) -> Vec<Subscription>>;

/// Fixed commands whose titles follow state: `observe` subscribes to that state
pub struct Group {
  id: SharedString,
  commands: Vec<Rc<dyn Command>>,
  observe: Option<ObserveFn>,
}

impl Group {
  pub fn new(id: impl Into<SharedString>, commands: Vec<Rc<dyn Command>>) -> Self {
    Self {
      id: id.into(),
      commands,
      observe: None,
    }
  }

  pub fn observe(
    mut self,
    observe: impl Fn(Notify, &mut App) -> Vec<Subscription> + 'static,
  ) -> Self {
    self.observe = Some(Rc::new(observe));
    self
  }

  pub fn register(self, cx: &mut App) {
    crate::registry::register_source(Rc::new(self), cx);
  }
}

impl CommandSource for Group {
  fn id(&self) -> SharedString {
    self.id.clone()
  }

  fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    self.commands.clone()
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    self
      .observe
      .as_ref()
      .map(|observe| observe(notify, cx))
      .unwrap_or_default()
  }
}

/// A list source from a closure, for pushed views
pub struct ListSource {
  id: SharedString,
  commands: Rc<dyn Fn(&Query, &App) -> Vec<Rc<dyn Command>>>,
  observe: Option<ObserveFn>,
  filters: bool,
  on_open: Option<Rc<dyn Fn(&mut App)>>,
  on_close: Option<Rc<dyn Fn(&mut App)>>,
  loading: Option<Rc<dyn Fn(&App) -> bool>>,
}

impl ListSource {
  pub fn new(
    id: impl Into<SharedString>,
    commands: impl Fn(&Query, &App) -> Vec<Rc<dyn Command>> + 'static,
  ) -> Self {
    Self {
      id: id.into(),
      commands: Rc::new(commands),
      observe: None,
      filters: false,
      on_open: None,
      on_close: None,
      loading: None,
    }
  }

  pub fn observe(
    mut self,
    observe: impl Fn(Notify, &mut App) -> Vec<Subscription> + 'static,
  ) -> Self {
    self.observe = Some(Rc::new(observe));
    self
  }

  /// The closure filters and orders for the query itself
  pub fn filters(mut self) -> Self {
    self.filters = true;
    self
  }

  pub fn on_open(mut self, f: impl Fn(&mut App) + 'static) -> Self {
    self.on_open = Some(Rc::new(f));
    self
  }

  pub fn on_close(mut self, f: impl Fn(&mut App) + 'static) -> Self {
    self.on_close = Some(Rc::new(f));
    self
  }

  pub fn loading(mut self, f: impl Fn(&App) -> bool + 'static) -> Self {
    self.loading = Some(Rc::new(f));
    self
  }
}

impl CommandSource for ListSource {
  fn id(&self) -> SharedString {
    self.id.clone()
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    (self.commands)(query, cx)
  }

  fn filters(&self) -> bool {
    self.filters
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    self
      .observe
      .as_ref()
      .map(|observe| observe(notify, cx))
      .unwrap_or_default()
  }

  fn loading(&self, cx: &App) -> bool {
    self.loading.as_ref().is_some_and(|f| f(cx))
  }

  fn open(&self, cx: &mut App) {
    if let Some(f) = &self.on_open {
      f(cx);
    }
  }

  fn close(&self, cx: &mut App) {
    if let Some(f) = &self.on_close {
      f(cx);
    }
  }
}

pub fn on_off(on: bool) -> SharedString {
  match on {
    true => t!("app.launcher.on").into(),
    false => t!("app.launcher.off").into(),
  }
}

pub fn tag(on: bool) -> Accessory {
  Accessory::Tag(on_off(on), if on { Tone::Ok } else { Tone::Off })
}

/// The state an `on`, `off` or `toggle` argument asks for, from `current`
pub fn switch_target(args: &Args, current: bool) -> bool {
  match args.text(0) {
    Some("on") => true,
    Some("off") => false,
    _ => !current,
  }
}

pub fn switch_arg() -> ArgSpec {
  ArgSpec::choice(t!("app.launcher.state"), ["on", "off", "toggle"]).optional()
}

/// A stateful switch: the title is what Enter does ("Disable Wi-Fi"), the subtitle and
/// tag show the state, `on`/`off`/`toggle` may follow inline
pub struct Toggle {
  pub id: &'static str,
  pub name: SharedString,
  pub icon: IconName,
  pub hue: f32,
  pub keywords: Vec<SharedString>,
  /// None when unavailable
  pub get: Rc<dyn Fn(&App) -> Option<bool>>,
  pub set: Rc<dyn Fn(bool, &mut App) -> Result<()>>,
}

impl Toggle {
  pub fn command(
    id: &'static str,
    name: impl Into<SharedString>,
    icon: IconName,
    hue: f32,
    get: impl Fn(&App) -> Option<bool> + 'static,
    set: impl Fn(bool, &mut App) -> Result<()> + 'static,
  ) -> Rc<dyn Command> {
    Rc::new(Self {
      id,
      name: name.into(),
      icon,
      hue,
      keywords: Vec::new(),
      get: Rc::new(get),
      set: Rc::new(set),
    })
  }
}

impl Command for Toggle {
  fn id(&self) -> SharedString {
    self.id.into()
  }

  fn meta(&self, cx: &App) -> Meta {
    let state = (self.get)(cx);
    let title = match state {
      Some(true) => t!("app.launcher.disable", name = self.name.clone()),
      Some(false) => t!("app.launcher.enable", name = self.name.clone()),
      None => self.name.to_string().into(),
    };
    let mut keywords = self.keywords.clone();
    keywords.push(self.name.clone());
    Meta {
      title: title.into(),
      subtitle: Some(match state {
        Some(on) => t!(
          "app.launcher.state_is",
          name = self.name.clone(),
          state = on_off(on)
        )
        .into(),
        None => t!("app.launcher.unavailable").into(),
      }),
      icon: Icon::Tile(self.icon, self.hue),
      keywords,
      accessories: state.map(tag).into_iter().collect(),
      args: vec![switch_arg()],
      ..Default::default()
    }
  }

  fn describe(&self, args: &Args, cx: &App) -> Option<SharedString> {
    let current = (self.get)(cx)?;
    let on = switch_target(args, current);
    Some(match on {
      true => t!("app.launcher.enable", name = self.name.clone()).into(),
      false => t!("app.launcher.disable", name = self.name.clone()).into(),
    })
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let (get, set, name) = (self.get.clone(), self.set.clone(), self.name.clone());
    vec![Action::sync(
      "toggle",
      t!("app.launcher.switch"),
      move |args, _, cx| {
        let current = get(cx).ok_or_else(|| anyhow!("{name} is unavailable"))?;
        let on = switch_target(args, current);
        set(on, cx)?;
        Ok(Outcome::Close(Some(Hud::new(t!(
          "app.launcher.state_is",
          name = name.clone(),
          state = on_off(on)
        )))))
      },
    )]
  }
}

/// How a level argument changes `current` (0..=1); `None` for a word like `mute`
pub fn level_target(value: &ArgValue, current: f32) -> Option<f32> {
  match value {
    ArgValue::Level(level) => Some(level.clamp(0., 1.)),
    ArgValue::Step(step) => Some((current + step).clamp(0., 1.)),
    _ => None,
  }
}

pub fn percent(level: f32) -> String {
  format!("{}%", (level * 100.).round() as i32)
}

/// Rows to pick one of, the current one tagged; `apply` runs on Enter, `preview` while
/// the selection moves, and the original comes back when the list closes without a pick
pub struct Choices {
  pub id: &'static str,
  pub items: Rc<dyn Fn(&App) -> Vec<(SharedString, SharedString)>>,
  pub current: Rc<dyn Fn(&App) -> Option<SharedString>>,
  pub apply: Rc<dyn Fn(&str, &mut App) -> Result<()>>,
  pub preview: Option<Rc<dyn Fn(&str, &mut App)>>,
  pub icon: Icon,
}

impl Choices {
  /// A list view of the items
  pub fn view(
    self,
    title: impl Into<SharedString>,
    placeholder: impl Into<SharedString>,
  ) -> crate::View {
    let this = Rc::new(self);
    let original: Rc<RefCell<Option<SharedString>>> = Rc::default();
    let picked: Rc<RefCell<bool>> = Rc::default();
    let (open_this, open_original, open_picked) = (this.clone(), original.clone(), picked.clone());
    let (close_this, close_original, close_picked) =
      (this.clone(), original.clone(), picked.clone());
    let source = ListSource::new(this.id, move |_, cx| {
      let current = (this.current)(cx);
      (this.items)(cx)
        .into_iter()
        .map(|(id, label)| {
          let is_current = current.as_ref() == Some(&id);
          let (apply, picked, value, icon) = (
            this.apply.clone(),
            picked.clone(),
            id.clone(),
            this.icon.clone(),
          );
          let preview = this.preview.clone();
          Rc::new(ChoiceRow {
            id: format!("{}:{}", this.id, id).into(),
            label,
            icon,
            current: is_current,
            value: value.clone(),
            apply,
            picked,
            preview,
          }) as Rc<dyn Command>
        })
        .collect()
    })
    .on_open(move |cx| {
      *open_original.borrow_mut() = (open_this.current)(cx);
      *open_picked.borrow_mut() = false;
    })
    .on_close(move |cx| {
      if *close_picked.borrow() || close_this.preview.is_none() {
        return;
      }
      if let Some(original) = close_original.borrow().clone() {
        let _ = (close_this.apply)(&original, cx);
      }
    });
    ListView::new(title, placeholder, Rc::new(source)).into_view()
  }
}

struct ChoiceRow {
  id: SharedString,
  label: SharedString,
  icon: Icon,
  current: bool,
  value: SharedString,
  apply: Rc<dyn Fn(&str, &mut App) -> Result<()>>,
  picked: Rc<RefCell<bool>>,
  preview: Option<Rc<dyn Fn(&str, &mut App)>>,
}

impl Command for ChoiceRow {
  fn id(&self) -> SharedString {
    self.id.clone()
  }

  fn meta(&self, _: &App) -> Meta {
    Meta {
      title: self.label.clone(),
      icon: self.icon.clone(),
      section: Section::Results,
      accessories: match self.current {
        true => vec![Accessory::Icon(IconName::Check)],
        false => Vec::new(),
      },
      ..Default::default()
    }
  }

  fn on_select(&self, cx: &mut App) {
    if let Some(preview) = &self.preview {
      preview(&self.value, cx);
    }
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let (apply, picked, value, label) = (
      self.apply.clone(),
      self.picked.clone(),
      self.value.clone(),
      self.label.clone(),
    );
    vec![Action::sync(
      "pick",
      t!("app.launcher.pick"),
      move |_, _, cx| {
        apply(&value, cx)?;
        *picked.borrow_mut() = true;
        Ok(Outcome::Close(Some(Hud::new(label.clone()))))
      },
    )]
  }
}

/// A command whose only action pushes a view built when it runs
pub fn opener(
  id: &'static str,
  title: impl Into<SharedString>,
  icon: Icon,
  keywords: &[&str],
  view: impl Fn(&App) -> crate::View + 'static,
) -> Rc<dyn Command> {
  let title: SharedString = title.into();
  let keywords: Vec<SharedString> = keywords
    .iter()
    .map(|k| SharedString::from(k.to_string()))
    .collect();
  let view = Rc::new(view);
  simple(
    id,
    move |_| Meta {
      title: title.clone(),
      icon: icon.clone(),
      keywords: keywords.clone(),
      ..Default::default()
    },
    move |_| {
      let view = view.clone();
      vec![Action::sync(
        "open",
        t!("app.launcher.open"),
        move |_, _, cx| Ok(Outcome::Push(view(cx))),
      )]
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn levels() {
    assert_eq!(level_target(&ArgValue::Level(0.4), 0.9), Some(0.4));
    assert_eq!(level_target(&ArgValue::Step(0.05), 0.98), Some(1.));
    assert_eq!(level_target(&ArgValue::Step(-0.1), 0.05), Some(0.));
    assert_eq!(level_target(&ArgValue::Word("mute".into()), 0.5), None);
    assert_eq!(percent(0.404), "40%");
  }

  #[test]
  fn switches() {
    let args = |s: &str| Args::parse(&[switch_arg()], s).unwrap();
    assert!(switch_target(&args("on"), true));
    assert!(!switch_target(&args("off"), false));
    assert!(switch_target(&args(""), false));
    assert!(!switch_target(&args("toggle"), true));
  }
}
