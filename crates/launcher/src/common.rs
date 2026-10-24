//! Actions every command has in its action panel: favorite, alias, hide, disable, configure

use std::rc::Rc;

use corona_config::{Config, ConfigProvider, launcher::CommandConfig};
use gpui_kit::{App, SharedString, Task, assets::IconName};
use rust_i18n::t;

use crate::{
  command::{Action, Outcome},
  hooks,
  view::{Field, FieldKind, FormView, View},
};

fn entry<'c>(c: &'c mut Config, id: &str) -> &'c mut CommandConfig {
  c.launcher.commands.entry(id.to_string()).or_default()
}

/// Changes a command's settings, dropping entries back at their defaults
pub fn update_command(id: &str, cx: &mut App, edit: impl FnOnce(&mut CommandConfig)) {
  let result = corona_config::update(cx, |c| {
    edit(entry(c, id));
    c.launcher
      .commands
      .retain(|_, e| *e != CommandConfig::default());
  });
  if let Err(e) = result {
    tracing::warn!("failed to save launcher settings: {e:#}");
  }
}

pub fn actions(id: SharedString, cx: &App) -> Vec<Action> {
  let current = cx
    .config()
    .launcher
    .command(&id)
    .cloned()
    .unwrap_or_default();
  let mut actions = Vec::new();

  let fav = id.clone();
  let (title, on) = match current.favorite {
    true => (t!("app.launcher.common.unfavorite"), false),
    false => (t!("app.launcher.common.favorite"), true),
  };
  actions.push(
    Action::sync("common:favorite", title, move |_, _, cx| {
      update_command(&fav, cx, |e| e.favorite = on);
      Ok(Outcome::KeepOpen)
    })
    .icon(IconName::Star)
    .shortcut("ctrl-shift-f")
    .keep_open(),
  );

  let alias_id = id.clone();
  let alias = current.alias.clone().unwrap_or_default();
  actions.push(
    Action::sync(
      "common:alias",
      t!("app.launcher.common.alias"),
      move |_, _, _| {
        let id = alias_id.clone();
        let submit = Rc::new(
          move |values: crate::view::FormValues, _: &mut gpui_kit::Window, cx: &mut App| {
            let alias = values.get("alias").cloned().unwrap_or_default();
            let alias = (!alias.trim().is_empty()).then(|| alias.trim().to_string());
            update_command(&id, cx, |e| e.alias = alias);
            Task::ready(Ok(Outcome::Pop))
          },
        );
        Ok(Outcome::Push(View::Form(FormView {
          title: t!("app.launcher.common.alias").into(),
          fields: vec![
            Field::new(
              "alias",
              t!("app.launcher.common.alias_field"),
              FieldKind::Text,
            )
            .value(alias.clone()),
          ],
          submit_label: t!("app.launcher.common.save").into(),
          submit,
          cancel: None,
          message: None,
        })))
      },
    )
    .icon(IconName::Tag),
  );

  let hide_id = id.clone();
  let hidden = current.hidden;
  actions.push(
    Action::sync(
      "common:hide",
      match hidden {
        true => t!("app.launcher.common.show"),
        false => t!("app.launcher.common.hide"),
      },
      move |_, _, cx| {
        update_command(&hide_id, cx, |e| e.hidden = !hidden);
        Ok(Outcome::KeepOpen)
      },
    )
    .icon(IconName::EyeOff)
    .keep_open(),
  );

  let disable_id = id.clone();
  actions.push(
    Action::sync(
      "common:disable",
      t!("app.launcher.common.disable"),
      move |_, _, cx| {
        update_command(&disable_id, cx, |e| e.enabled = false);
        Ok(Outcome::KeepOpen)
      },
    )
    .icon(IconName::Ban)
    .keep_open(),
  );

  actions.push(
    Action::sync(
      "common:configure",
      t!("app.launcher.common.configure"),
      |_, _, cx| {
        hooks::open_settings(Some("launcher"), cx);
        Ok(Outcome::close())
      },
    )
    .icon(IconName::Settings),
  );
  actions
}
