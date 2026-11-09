//! A plugin's rows and pages as launcher commands and views

use std::{cell::RefCell, path::Path, rc::Rc};

use anyhow::Result;
use corona_launcher::{
  Accessory, Action, Clip, Command, CommandSource, Confirm, Detail, DetailBody, DetailView, Field,
  FieldKind, FormView, Hud, Icon, ListView, Meta, Outcome, Query, Section, Toast, ToastKind, Tone,
  View, command::Scope, detail_row, icon_by_name, kit::Notify,
};
use gpui_kit::{App, SharedString, Subscription, Window};
use serde_json::{Value, json};

use super::{
  Port,
  page::Page,
  spec::{self, PageSpec, Row},
};

fn tone(tone: Option<spec::Tone>) -> Tone {
  match tone {
    Some(spec::Tone::Ok) => Tone::Ok,
    Some(spec::Tone::Warn) => Tone::Warn,
    Some(spec::Tone::Err) => Tone::Err,
    _ => Tone::Off,
  }
}

fn clip(clip: spec::Clip) -> Result<Clip> {
  use base64::{Engine, engine::general_purpose::STANDARD};
  Ok(match clip {
    spec::Clip::Text { text } => Clip::Text(text),
    spec::Clip::Image { image, mime } => Clip::Image {
      bytes: std::sync::Arc::new(STANDARD.decode(image)?),
      mime: mime.unwrap_or_else(|| "image/png".into()),
    },
  })
}

fn failure(error: impl std::fmt::Display) -> Outcome {
  Outcome::Toast(Toast {
    kind: ToastKind::Failure,
    title: format!("{error:#}").into(),
    message: None,
    action: None,
  })
}

/// What the launcher does for the answer of a handler; a failure is a toast
pub(crate) fn outcome(answer: Result<Value>, port: &Port) -> Outcome {
  use spec::Reply;
  let run = || -> Result<Outcome> {
    let answer = answer?;
    // nothing, or an empty object
    if answer.is_null() || answer.as_object().is_some_and(|o| o.is_empty()) {
      return Ok(Outcome::close());
    }
    Ok(match serde_json::from_value::<Reply>(answer)? {
      Reply::Push(page) => {
        let page = port
          .page(page.id)
          .ok_or_else(|| anyhow::anyhow!("the page is closed"))?;
        Outcome::Push(view(page))
      }
      Reply::Pop(true) => Outcome::Pop,
      Reply::KeepOpen(true) => Outcome::KeepOpen,
      Reply::Pop(false) | Reply::KeepOpen(false) => Outcome::close(),
      Reply::Toast(toast) => Outcome::Toast(Toast {
        kind: match toast.kind {
          Some(spec::ToastKind::Failure) => ToastKind::Failure,
          Some(spec::ToastKind::Progress) => ToastKind::Progress,
          _ => ToastKind::Success,
        },
        title: toast.title.into(),
        message: toast.message.map(Into::into),
        action: None,
      }),
      Reply::Copy(c) => Outcome::Copy(clip(c)?),
      Reply::Paste(c) => Outcome::Paste(clip(c)?),
      Reply::Hud(hud) => {
        let mut pill = Hud::new(hud.text);
        pill.icon = hud.icon.as_deref().and_then(icon_by_name);
        Outcome::Close(Some(pill))
      }
    })
  };
  run().unwrap_or_else(failure)
}

fn icon(spec: &spec::Icon, dir: &Path) -> Icon {
  use spec::Icon as I;
  match spec {
    I::Name(name) => icon_by_name(name).map_or(Icon::None, Icon::Named),
    I::App(names) => Icon::App(names.iter().map(|n| n.clone().into()).collect()),
    I::Glyph(glyph) => Icon::Glyph(glyph.clone().into()),
    I::Swatch(hex) => u32::from_str_radix(hex.trim_start_matches('#'), 16)
      .map_or(Icon::None, |rgb| Icon::Swatch(gpui_kit::rgb(rgb).into())),
    I::Image(path) => inside(dir, path).map_or(Icon::None, |p| Icon::Image(p.into())),
  }
}

/// `path` in the plugin's directory
// ponytail: the plugin's own directory only, not the files its `fs` grant reaches
fn inside(dir: &Path, path: &str) -> Option<std::path::PathBuf> {
  let full = dir.join(path).canonicalize().ok()?;
  full.starts_with(dir.canonicalize().ok()?).then_some(full)
}

fn preview(spec: &spec::Preview, dir: &Path) -> Detail {
  let body = match (&spec.text, &spec.code, &spec.image) {
    (Some(text), ..) => DetailBody::Text(text.clone().into()),
    (_, Some(code), _) => DetailBody::Code(code.clone().into()),
    (.., Some(image)) => {
      inside(dir, image).map_or(DetailBody::None, |p| DetailBody::Image(p.into()))
    }
    _ => DetailBody::None,
  };
  Detail {
    body,
    metadata: (spec.metadata.iter())
      .map(|(k, v)| detail_row(k.clone(), v.clone()))
      .collect(),
    tags: (spec.tags.iter())
      .map(|(text, t)| (text.clone().into(), tone(*t)))
      .collect(),
  }
}

/// The action that asks the plugin's handler and does what it answers with
pub(super) fn action(
  spec: &spec::Action,
  args: impl Fn() -> Value + 'static,
  port: &Port,
) -> Action {
  let (handler, port) = (spec.run.id, port.clone());
  let mut action = Action::spawn(
    format!("handler-{}", spec.run.id),
    spec.title.clone(),
    move |_, cx| {
      let answer = port.request(handler, args(), cx);
      let port = port.clone();
      async move {
        let answer = match answer {
          Ok(answer) => answer.await,
          Err(e) => Err(e),
        };
        Ok(outcome(answer, &port))
      }
    },
  );
  action.icon = spec.icon.as_deref().and_then(icon_by_name);
  action.shortcut = spec.shortcut.clone().map(Into::into);
  action.destructive = spec.destructive;
  action.keep_open = spec.keep_open;
  action.confirm = spec.confirm.as_ref().map(|c| Confirm {
    title: c.title.clone().into(),
    message: c.message.clone().into(),
    button: c.button.clone().into(),
    skippable: c.skippable,
  });
  action
}

/// One row of a plugin's list
struct RowCommand {
  plugin: SharedString,
  row: Row,
  port: Port,
}

impl Command for RowCommand {
  fn id(&self) -> SharedString {
    format!("{}:row:{}", self.plugin, self.row.id).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let dir = self.port.dir();
    let row = &self.row;
    Meta {
      title: row.title.clone().into(),
      subtitle: row.subtitle.clone().map(Into::into),
      icon: row.icon.as_ref().map_or(Icon::None, |i| icon(i, &dir)),
      keywords: row.keywords.iter().map(|k| k.clone().into()).collect(),
      section: Section::Results,
      accessories: row
        .accessories
        .iter()
        .filter_map(|a| {
          Some(match a {
            spec::Accessory::Tag { text, tone: t } => Accessory::Tag(text.clone().into(), tone(*t)),
            spec::Accessory::Text(text) => Accessory::Text(text.clone().into()),
            spec::Accessory::Icon(name) => Accessory::Icon(icon_by_name(name)?),
            spec::Accessory::Progress(p) => Accessory::Progress(*p),
          })
        })
        .collect(),
      ..Meta::default()
    }
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    (self.row.actions.iter())
      .map(|spec| action(spec, || json!({}), &self.port))
      .collect()
  }

  fn detail(&self, _: &App) -> Option<Detail> {
    let dir = self.port.dir();
    self.row.detail.as_ref().map(|d| preview(d, &dir))
  }
}

/// The rows of a list page
pub(super) struct PageSource {
  pub plugin: SharedString,
  pub page: Rc<Page>,
}

impl CommandSource for PageSource {
  fn id(&self) -> SharedString {
    format!("{}:page", self.plugin).into()
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    self.page.sync(&query.text);
    (self.page.rows().into_iter())
      .map(|row| {
        Rc::new(RowCommand {
          plugin: self.plugin.clone(),
          row,
          port: self.page.port.clone(),
        }) as Rc<dyn Command>
      })
      .collect()
  }

  fn filters(&self) -> bool {
    self.page.filters()
  }

  fn observe(&self, notify: Notify, _: &mut App) -> Vec<Subscription> {
    let (page, id) = (self.page.clone(), self.page.observe(notify));
    vec![Subscription::new(move || page.unobserve(id))]
  }

  fn loading(&self, _: &App) -> bool {
    self.page.loading()
  }
}

/// The page as a launcher view
pub(super) fn view(page: Rc<Page>) -> View {
  let port = page.port.clone();
  match &page.spec {
    PageSpec::Form(spec) => View::Form(form(spec, port)),
    PageSpec::Detail(spec) => View::Detail(DetailView {
      title: spec.title.clone().into(),
      markdown: spec.markdown.clone().into(),
      metadata: (spec.metadata.iter())
        .map(|(k, v)| detail_row(k.clone(), v.clone()))
        .collect(),
      actions: (spec.actions.iter())
        .map(|a| action(a, || json!({}), &port))
        .collect(),
    }),
    PageSpec::List(spec) => {
      let source = Rc::new(PageSource {
        plugin: page.port.id().into(),
        page: page.clone(),
      });
      let placeholder = spec.placeholder.clone().unwrap_or_default();
      let mut list = ListView::new(spec.title.clone(), placeholder, source);
      if let Some(grid) = &spec.grid {
        list = list.grid((grid.columns as usize).max(1), grid.cell, grid.titles);
      }
      if spec.preview {
        list = list.preview();
      }
      if let Some(filter) = &spec.filter {
        list = list.filter(corona_launcher::Filter {
          label: filter.label.clone().into(),
          options: (filter.options.iter())
            .map(|o| (o.id.clone().into(), o.title.clone().into()))
            .collect(),
          selected: page.filter.clone(),
        });
      }
      if let Some(empty) = &spec.empty {
        list = list.empty(empty.clone());
      }
      list
        .query(spec.query.clone().unwrap_or_default())
        .into_view()
    }
  }
}

fn form(spec: &spec::FormSpec, port: Port) -> FormView {
  let fields = spec.fields.iter().map(field).collect();
  let submit_port = port.clone();
  let on_submit = spec.on_submit.id;
  let submit: corona_launcher::view::Submit =
    Rc::new(move |values, _: &mut Window, cx: &mut App| {
      let port = submit_port.clone();
      let values: serde_json::Map<String, Value> = values
        .into_iter()
        .map(|(k, v)| (k.to_string(), Value::String(v)))
        .collect();
      let answer = port.request(on_submit, Value::Object(values), cx);
      cx.spawn(async move |_| {
        let answer = match answer {
          Ok(answer) => answer.await,
          Err(e) => Err(e),
        };
        Ok(outcome(answer, &port))
      })
    });
  let cancel = spec
    .on_cancel
    .map(|handler| -> corona_launcher::view::Cancel {
      Rc::new(move |_| port.call(handler.id, json!(null), Box::new(|_, _| {})))
    });
  FormView {
    title: spec.title.clone().into(),
    fields,
    submit_label: spec
      .submit_label
      .clone()
      .unwrap_or_else(|| "Submit".into())
      .into(),
    submit,
    cancel,
    message: spec.message.clone().map(Into::into),
  }
}

fn field(spec: &spec::Field) -> Field {
  let kind = match spec.kind {
    Some(spec::FieldKind::Password) => FieldKind::Password,
    Some(spec::FieldKind::Textarea) => FieldKind::TextArea,
    Some(spec::FieldKind::Dropdown) => FieldKind::Dropdown(
      (spec.options.iter())
        .map(|o| (o.id.clone().into(), o.title.clone().into()))
        .collect(),
    ),
    _ => FieldKind::Text,
  };
  let mut field = Field::new(spec.id.clone(), spec.label.clone(), kind)
    .value(spec.value.clone().unwrap_or_default())
    .placeholder(spec.placeholder.clone().unwrap_or_default());
  if spec.required {
    field = field.required();
  }
  field
}

/// The command a manifest declares: it runs the service's `onRun` handler of its name
pub(super) struct PluginCommand {
  pub id: SharedString,
  pub name: String,
  pub meta: Meta,
  pub scope: Option<(String, String, String)>,
  pub port: Port,
}

impl PluginCommand {
  /// Runs the handler and what it answers with
  fn run(port: &Port, name: &str, args: Value, cx: &App) -> impl Future<Output = Outcome> + use<> {
    let answer = match port.command(name) {
      Some(handler) => port.request(handler, args, cx),
      None => Err(anyhow::anyhow!("the plugin has not registered `{name}`")),
    };
    let port = port.clone();
    async move {
      let answer = match answer {
        Ok(answer) => answer.await,
        Err(e) => Err(e),
      };
      outcome(answer, &port)
    }
  }
}

impl PluginCommand {
  /// Runs the handler, which answers with the list page to search
  fn run_page(port: &Port, name: &str, cx: &App) -> impl Future<Output = Option<Rc<Page>>> + use<> {
    let answer = port
      .command(name)
      .map(|handler| port.request(handler, json!([]), cx));
    let port = port.clone();
    async move {
      let value = answer?.ok()?.await.ok()?;
      match serde_json::from_value(value).ok()? {
        spec::Reply::Push(page) => port.page(page.id),
        _ => None,
      }
    }
  }
}

fn args_json(args: &corona_launcher::Args) -> Value {
  use corona_launcher::ArgValue;
  Value::Array(
    (args.0.iter())
      .map(|arg| match arg {
        None => Value::Null,
        Some(ArgValue::Number(n)) => json!(n),
        Some(ArgValue::Level(n) | ArgValue::Step(n)) => json!(n),
        Some(other) => other.as_str().map_or(Value::Null, |s| json!(s)),
      })
      .collect(),
  )
}

impl Command for PluginCommand {
  fn id(&self) -> SharedString {
    self.id.clone()
  }

  fn meta(&self, _: &App) -> Meta {
    self.meta.clone()
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let (port, name) = (self.port.clone(), self.name.clone());
    let title = self.meta.title.clone();
    vec![Action::spawn("run", title, move |args, cx| {
      let run = Self::run(&port, &name, args_json(args), cx);
      async move { Ok(run.await) }
    })]
  }

  fn scope(&self, _: &App) -> Option<Scope> {
    let (prefix, label, placeholder) = self.scope.clone()?;
    Some(Scope {
      prefix: prefix.into(),
      label: label.into(),
      placeholder: placeholder.into(),
      source: Rc::new(ScopeSource {
        plugin: self.id.clone(),
        name: self.name.clone(),
        port: self.port.clone(),
        inner: Rc::default(),
        notifier: Rc::default(),
      }),
    })
  }
}

/// A command's list page as the search of its prefix: opening it runs the handler
struct ScopeSource {
  plugin: SharedString,
  name: String,
  port: Port,
  inner: Rc<RefCell<Option<Rc<PageSource>>>>,
  notifier: Rc<RefCell<Option<Notify>>>,
}

impl CommandSource for ScopeSource {
  fn id(&self) -> SharedString {
    self.plugin.clone()
  }

  fn filters(&self) -> bool {
    (self.inner.borrow().as_ref()).is_some_and(|s| s.filters())
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let source = self.inner.borrow().clone();
    source.map_or_else(Vec::new, |s| s.commands(query, cx))
  }

  fn loading(&self, cx: &App) -> bool {
    (self.inner.borrow().as_ref()).is_none_or(|s| s.loading(cx))
  }

  fn observe(&self, notify: Notify, _: &mut App) -> Vec<Subscription> {
    // the page's own notifier is hooked as it opens
    self.notifier.replace(Some(notify));
    Vec::new()
  }

  fn open(&self, cx: &mut App) {
    let run = PluginCommand::run_page(&self.port, &self.name, cx);
    let (inner, notify) = (self.inner.clone(), self.notifier.clone());
    let plugin = self.plugin.clone();
    cx.spawn(async move |cx| {
      if let Some(page) = run.await {
        *inner.borrow_mut() = Some(Rc::new(PageSource {
          plugin,
          page: page.clone(),
        }));
        let notify = notify.borrow().clone();
        if let Some(notify) = notify {
          page.observe(notify.clone());
          cx.update(|cx| notify(cx));
        }
      }
    })
    .detach();
  }
}

#[cfg(test)]
mod tests {
  use anyhow::anyhow;

  use super::*;

  #[test]
  fn replies_become_outcomes() {
    let port = Port::new("p");
    let outcome = |v: Value| outcome(Ok(v), &port);
    assert!(matches!(outcome(Value::Null), Outcome::Close(None)));
    assert!(matches!(outcome(json!({})), Outcome::Close(None)));
    assert!(matches!(
      outcome(json!({"hud": {"text": "x"}})),
      Outcome::Close(Some(_))
    ));
    assert!(matches!(outcome(json!({"pop": true})), Outcome::Pop));
    assert!(matches!(
      outcome(json!({"keepOpen": true})),
      Outcome::KeepOpen
    ));
    assert!(matches!(
      outcome(json!({"copy": {"text": "a"}})),
      Outcome::Copy(Clip::Text(_))
    ));
    // a page that is gone, a reply that is nonsense, and a handler that failed are toasts
    for bad in [
      json!({"push": {"id": 9}}),
      json!({"toast": 3}),
      json!({"pop": true, "keepOpen": true}),
    ] {
      let shown = bad.to_string();
      let Outcome::Toast(toast) = outcome(bad) else {
        panic!("{shown} is not a failure")
      };
      assert_eq!(toast.kind, ToastKind::Failure);
    }
    let Outcome::Toast(toast) = super::outcome(Err(anyhow!("boom")), &port) else {
      panic!()
    };
    assert_eq!(toast.title, "boom");
  }
}
