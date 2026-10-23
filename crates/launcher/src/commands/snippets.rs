//! Snippets (pasted text) and quicklinks (opened URLs or paths), with placeholders

use std::{collections::HashMap, rc::Rc};

use corona_clipboard::ClipboardExt;
use corona_config::{
  ConfigProvider,
  launcher::{Quicklink, Snippet},
};
use gpui_kit::{App, SharedString, Task, assets::IconName};
use jiff::Zoned;
use rust_i18n::t;

use crate::{
  args::{ArgSpec, Args},
  command::{Action, Clip, Command, CommandSource, Confirm, Icon, Meta, Outcome},
  commands::calculator::calculate,
  kit::{ListSource, opener},
  matching::{Match, Query, Tier, default_match},
  placeholders::{self, Env},
  registry,
  view::{Field, FieldKind, FormValues, FormView, ListView, View},
};

/// Argument values by placeholder name, from inline text or the arguments row
fn values(template: &str, args: &Args) -> HashMap<String, String> {
  placeholders::arguments(template)
    .into_iter()
    .enumerate()
    .filter_map(|(i, p)| Some((p.attrs.get("name")?.clone(), args.text(i)?.to_string())))
    .collect()
}

fn expand(template: &str, args: &Args, url: bool, cx: &App) -> String {
  let clipboard = |offset: usize| match offset {
    0 => cx
      .read_from_clipboard()
      .and_then(|item| item.text())
      .or_else(|| cx.clipboard_history()?.nth_text(0)),
    n => cx.clipboard_history()?.nth_text(n),
  };
  let snippets = cx.config().launcher.snippets.clone();
  let snippet = move |name: &str| {
    snippets
      .iter()
      .find(|s| s.name.eq_ignore_ascii_case(name))
      .map(|s| s.text.clone())
  };
  let calc = |expression: &str| calculate(expression, cx);
  placeholders::expand(
    template,
    &Env {
      now: Zoned::now(),
      clipboard: &clipboard,
      selection: cx.read_from_primary().and_then(|item| item.text()),
      arguments: values(template, args),
      snippet: &snippet,
      calculate: &calc,
      url,
    },
  )
}

/// The argument fields a template asks for
fn specs(template: &str) -> Vec<ArgSpec> {
  placeholders::arguments(template)
    .into_iter()
    .map(|p| {
      let name = p.attrs.get("name").cloned().unwrap_or_default();
      let spec = match p.attrs.get("options") {
        Some(options) => ArgSpec::choice(
          name,
          options
            .split(',')
            .map(|o| SharedString::from(o.trim().to_string()))
            .collect::<Vec<_>>(),
        ),
        None => ArgSpec::text(name),
      };
      match p.attrs.contains_key("default") {
        true => spec.optional(),
        false => spec,
      }
    })
    .collect()
}

struct SnippetCommand(Snippet);

impl Command for SnippetCommand {
  fn id(&self) -> SharedString {
    format!("snippet:{}", self.0.name).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let args = specs(&self.0.text);
    Meta {
      title: self.0.name.clone().into(),
      subtitle: Some(
        self
          .0
          .text
          .lines()
          .next()
          .unwrap_or_default()
          .to_string()
          .into(),
      ),
      icon: Icon::Tile(IconName::TextCursorInput, 90.),
      keywords: self.0.keyword.iter().cloned().map(Into::into).collect(),
      args_row: args.len() > 1,
      args,
      ..Default::default()
    }
  }

  /// Typing the keyword offers the snippet first
  fn matches(&self, query: &Query, cx: &App) -> Option<Match> {
    if let Some(keyword) = &self.0.keyword
      && query.text == keyword.to_lowercase()
    {
      return Some(Match::new(Tier::Alias));
    }
    default_match(&self.meta(cx), &[], query)
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let (paste, copy) = (self.0.text.clone(), self.0.text.clone());
    vec![
      Action::sync(
        "paste",
        t!("app.launcher.snippets.paste"),
        move |args, _, cx| Ok(Outcome::Paste(Clip::Text(expand(&paste, args, false, cx)))),
      ),
      Action::sync(
        "copy",
        t!("app.launcher.snippets.copy"),
        move |args, _, cx| Ok(Outcome::Copy(Clip::Text(expand(&copy, args, false, cx)))),
      )
      .shortcut("ctrl-enter"),
    ]
  }
}

struct QuicklinkCommand(Quicklink);

impl Command for QuicklinkCommand {
  fn id(&self) -> SharedString {
    format!("quicklink:{}", self.0.name).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let args = specs(&self.0.url);
    Meta {
      title: self.0.name.clone().into(),
      subtitle: Some(self.0.url.clone().into()),
      icon: Icon::Tile(IconName::Link, 200.),
      args_row: args.len() > 1,
      args,
      ..Default::default()
    }
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let url = self.0.url.clone();
    let open = Action::sync("open", t!("app.launcher.open"), move |args, _, cx| {
      let target = expand(&url, args, url.contains("://"), cx);
      match target.strip_prefix("~/") {
        Some(rest) => {
          let home = dirs::home_dir().unwrap_or_default();
          cx.open_with_system(&home.join(rest));
        }
        None if target.starts_with('/') => cx.open_with_system(std::path::Path::new(&target)),
        None => cx.open_url(&target),
      }
      Ok(Outcome::close())
    });
    let (edit, delete) = (self.0.clone(), self.0.name.clone());
    vec![
      open,
      Action::sync(
        "edit",
        t!("app.launcher.quicklinks.edit"),
        move |_, _, _| Ok(Outcome::Push(quicklink_form(Some(edit.clone())))),
      ),
      Action::sync(
        "delete",
        t!("app.launcher.quicklinks.delete"),
        move |_, _, cx| {
          let name = delete.clone();
          save(cx, |c| c.launcher.quicklinks.retain(|q| q.name != name))?;
          Ok(Outcome::KeepOpen)
        },
      )
      .destructive()
      .keep_open(),
    ]
  }
}

/// Every quicklink and snippet from the config
struct Templates;

impl CommandSource for Templates {
  fn id(&self) -> SharedString {
    "templates".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let launcher = &cx.config().launcher;
    launcher
      .quicklinks
      .iter()
      .map(|q| Rc::new(QuicklinkCommand(q.clone())) as Rc<dyn Command>)
      .chain(
        launcher
          .snippets
          .iter()
          .map(|s| Rc::new(SnippetCommand(s.clone())) as Rc<dyn Command>),
      )
      .collect()
  }
}

fn save(cx: &mut App, edit: impl FnOnce(&mut corona_config::Config)) -> anyhow::Result<()> {
  corona_config::update(cx, edit)
}

fn quicklink_form(existing: Option<Quicklink>) -> View {
  let current = existing.clone().unwrap_or_default();
  View::Form(FormView {
    title: match existing {
      Some(_) => t!("app.launcher.quicklinks.edit"),
      None => t!("app.launcher.quicklinks.create"),
    }
    .into(),
    fields: vec![
      Field::new(
        "name",
        t!("app.settings.launcher.fields.name"),
        FieldKind::Text,
      )
      .required()
      .value(current.name.clone()),
      Field::new(
        "url",
        t!("app.settings.launcher.fields.url"),
        FieldKind::Text,
      )
      .required()
      .placeholder("https://search.nixos.org/packages?query={argument}")
      .value(current.url.clone()),
      Field::new(
        "icon",
        t!("app.settings.launcher.fields.icon"),
        FieldKind::Text,
      )
      .value(current.icon.clone().unwrap_or_default()),
    ],
    submit_label: t!("app.launcher.common.save").into(),
    submit: Rc::new(move |values: FormValues, _, cx| {
      let link = Quicklink {
        name: values["name"].trim().to_string(),
        url: values["url"].trim().to_string(),
        icon: Some(values["icon"].trim().to_string()).filter(|i| !i.is_empty()),
      };
      let old = existing.as_ref().map(|q| q.name.clone());
      Task::ready(
        save(cx, |c| {
          let list = &mut c.launcher.quicklinks;
          match old.and_then(|name| list.iter().position(|q| q.name == name)) {
            Some(i) => list[i] = link,
            None => list.push(link),
          }
        })
        .map(|_| Outcome::hud(t!("app.launcher.quicklinks.saved"))),
      )
    }),
    cancel: None,
    message: None,
  })
}

fn snippet_form(existing: Option<Snippet>, copy: bool) -> View {
  let current = existing.clone().unwrap_or_default();
  View::Form(FormView {
    title: match (existing.is_some(), copy) {
      (true, false) => t!("app.launcher.snippets.edit"),
      _ => t!("app.launcher.snippets.create"),
    }
    .into(),
    fields: vec![
      Field::new(
        "name",
        t!("app.settings.launcher.fields.name"),
        FieldKind::Text,
      )
      .required()
      .value(match copy {
        true => format!("{} 2", current.name),
        false => current.name.clone(),
      }),
      Field::new(
        "keyword",
        t!("app.settings.launcher.fields.keyword"),
        FieldKind::Text,
      )
      .placeholder(";sig")
      .value(current.keyword.clone().unwrap_or_default()),
      Field::new(
        "text",
        t!("app.settings.launcher.fields.text"),
        FieldKind::TextArea,
      )
      .required()
      .value(current.text.clone()),
    ],
    submit_label: t!("app.launcher.common.save").into(),
    submit: Rc::new(move |values: FormValues, _, cx| {
      let snippet = Snippet {
        name: values["name"].trim().to_string(),
        keyword: Some(values["keyword"].trim().to_string()).filter(|k| !k.is_empty()),
        text: values["text"].clone(),
      };
      let old = existing.as_ref().filter(|_| !copy).map(|s| s.name.clone());
      Task::ready(
        save(cx, |c| {
          let list = &mut c.launcher.snippets;
          match old.and_then(|name| list.iter().position(|s| s.name == name)) {
            Some(i) => list[i] = snippet,
            None => list.push(snippet),
          }
        })
        .map(|_| Outcome::hud(t!("app.launcher.snippets.saved"))),
      )
    }),
    cancel: None,
    message: None,
  })
}

/// Snippets with edit, duplicate and delete
fn manage_view() -> View {
  ListView::new(
    t!("app.launcher.snippets.search_title"),
    t!("app.launcher.snippets.search"),
    Rc::new(ListSource::new("snippets:manage", |_, cx| {
      cx.config()
        .launcher
        .snippets
        .iter()
        .map(|s| {
          let s = s.clone();
          Rc::new(ManagedSnippet(s)) as Rc<dyn Command>
        })
        .collect()
    })),
  )
  .empty(t!("app.launcher.snippets.none"))
  .into_view()
}

struct ManagedSnippet(Snippet);

impl Command for ManagedSnippet {
  fn id(&self) -> SharedString {
    format!("snippet-manage:{}", self.0.name).into()
  }

  fn meta(&self, cx: &App) -> Meta {
    SnippetCommand(self.0.clone()).meta(cx)
  }

  fn detail(&self, _: &App) -> Option<crate::Detail> {
    Some(crate::Detail {
      body: crate::DetailBody::Code(self.0.text.clone().into()),
      metadata: self
        .0
        .keyword
        .iter()
        .map(|k| {
          (
            t!("app.settings.launcher.fields.keyword").into(),
            k.clone().into(),
          )
        })
        .collect(),
      tags: Vec::new(),
    })
  }

  fn actions(&self, cx: &App) -> Vec<Action> {
    let mut actions = SnippetCommand(self.0.clone()).actions(cx);
    let (edit, duplicate, delete) = (self.0.clone(), self.0.clone(), self.0.name.clone());
    actions.push(Action::sync(
      "edit",
      t!("app.launcher.snippets.edit"),
      move |_, _, _| Ok(Outcome::Push(snippet_form(Some(edit.clone()), false))),
    ));
    actions.push(Action::sync(
      "duplicate",
      t!("app.launcher.snippets.duplicate"),
      move |_, _, _| Ok(Outcome::Push(snippet_form(Some(duplicate.clone()), true))),
    ));
    actions.push(
      Action::sync(
        "delete",
        t!("app.launcher.snippets.delete"),
        move |_, _, cx| {
          let name = delete.clone();
          save(cx, |c| c.launcher.snippets.retain(|s| s.name != name))?;
          Ok(Outcome::KeepOpen)
        },
      )
      .destructive()
      .keep_open()
      .confirm(Confirm {
        title: t!("app.launcher.snippets.delete").into(),
        message: t!(
          "app.launcher.snippets.delete_message",
          name = self.0.name.clone()
        )
        .into(),
        button: t!("app.launcher.snippets.delete").into(),
        skippable: false,
      }),
    );
    actions
  }
}

pub fn init(cx: &mut App) {
  registry::register_source(Rc::new(Templates), cx);
  registry::register_all(
    [
      opener(
        "quicklinks:create",
        t!("app.launcher.quicklinks.create"),
        Icon::Tile(IconName::Link, 200.),
        &["quicklink", "bookmark", "url"],
        |_| quicklink_form(None),
      ),
      opener(
        "snippets:create",
        t!("app.launcher.snippets.create"),
        Icon::Tile(IconName::TextCursorInput, 90.),
        &["snippet", "text", "template"],
        |_| snippet_form(None, false),
      ),
      opener(
        "snippets:search",
        t!("app.launcher.snippets.search_title"),
        Icon::Tile(IconName::TextSearch, 90.),
        &["snippet", "templates"],
        |_| manage_view(),
      ),
    ],
    cx,
  );
}
