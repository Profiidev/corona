use std::borrow::Cow;

use corona_config::{
  Config, ConfigProvider,
  plugins::{AutoUpdate, OFFICIAL_SOURCE, SourceConfig, SourceKind},
};
use corona_script::{
  PluginManager, PluginStatus, ScriptManager,
  plugin::{
    manifest::PluginManifest,
    settings::{DynamicOptions, Setting, SettingKind, number as json_number},
  },
  secrets,
};
use gpui_kit::{
  App, AppContext, Context, Div, Entity, Focusable, IntoElement, ParentElement, SharedString,
  Styled, Subscription, Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
    spinner::Spinner,
    switch::Switch,
    tag::{Tag, TagVariant},
    text::TextView,
  },
  div, img,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;
use serde_json::Value;

use crate::settings::fields::{Choice, choice, number, save, searchable, slider, switch, text};

pub(super) fn page(cx: &App) -> SettingPage {
  let mut groups = vec![sources_group(cx), plugins_group(cx)];
  if let Some(manager) = cx.try_global::<PluginManager>() {
    let mut running: Vec<PluginManifest> = manager
      .active()
      .values()
      .map(|found| found.manifest.clone())
      .filter(|m| !m.settings.is_empty())
      .collect();
    running.sort_by(|a, b| a.name.cmp(&b.name));
    groups.extend(running.iter().map(|m| settings_group(m, cx)));
  }
  SettingPage::new(t!("app.settings.plugins.title"))
    .icon(Icon::new(IconName::LayoutDashboard))
    .resettable(true)
    .groups(groups)
}

fn sources_group(cx: &App) -> SettingGroup {
  let mut items: Vec<SettingItem> = cx
    .config()
    .plugins
    .source
    .iter()
    .map(|source| {
      let name = source.name.clone();
      SettingItem::render(move |_, _, cx| source_row(&name, cx))
    })
    .collect();
  items.push(SettingItem::render(|_, window, cx| add_source(window, cx)));
  items.push(
    SettingItem::new(
      t!("app.settings.plugins.auto_update.title"),
      choice(
        &[
          (AutoUpdate::All, t!("app.settings.plugins.auto_update.all")),
          (
            AutoUpdate::Official,
            t!("app.settings.plugins.auto_update.official"),
          ),
          (
            AutoUpdate::None,
            t!("app.settings.plugins.auto_update.none"),
          ),
        ],
        |c| c.plugins.auto_update,
        |c, v| c.plugins.auto_update = v,
      ),
    )
    .description(t!("app.settings.plugins.auto_update.description").into_owned()),
  );
  SettingGroup::new()
    .title(t!("app.settings.plugins.groups.sources"))
    .items(items)
}

fn kind_label(kind: SourceKind) -> Cow<'static, str> {
  match kind {
    SourceKind::Git => t!("app.settings.plugins.kind.git"),
    SourceKind::Path => t!("app.settings.plugins.kind.path"),
    SourceKind::Dev => t!("app.settings.plugins.kind.dev"),
  }
}

fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
  div()
    .text_xs()
    .text_color(cx.theme().muted_foreground)
    .child(text.into())
}

fn error_text(error: Option<&str>, cx: &App) -> Option<Div> {
  error.map(|e| {
    div()
      .text_xs()
      .text_color(cx.theme().danger)
      .child(e.to_string())
  })
}

fn source_row(name: &str, cx: &App) -> Div {
  let Some(source) = cx
    .config()
    .plugins
    .source
    .iter()
    .find(|s| s.name == name)
    .cloned()
  else {
    return div();
  };
  let manager = cx.try_global::<PluginManager>();
  let busy = manager.is_some_and(|m| m.is_busy(name));
  let error = manager
    .and_then(|m| m.source_error(name))
    .map(str::to_string);
  let (toggle, update, remove) = (name.to_string(), name.to_string(), name.to_string());
  div()
    .flex()
    .items_center()
    .gap_2()
    .child(
      div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .child(
          div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().text_sm().child(source.name.clone()))
            .child(
              Tag::new()
                .small()
                .with_variant(TagVariant::Secondary)
                .child(kind_label(source.kind)),
            ),
        )
        .child(muted(source.location.clone(), cx).truncate())
        .children(error_text(error.as_deref(), cx)),
    )
    .when(busy, |d| d.child(Spinner::new()))
    .when(source.kind == SourceKind::Git, |d| {
      d.child(
        Button::new(format!("source-update-{name}"))
          .label(t!("app.settings.plugins.update"))
          .icon(IconName::RotateCw)
          .cursor_pointer()
          .outline()
          .disabled(busy || !source.enabled)
          .on_click(move |_, _, cx| PluginManager::update_source(&update, cx)),
      )
    })
    .child(
      Switch::new(format!("source-enabled-{name}"))
        .checked(source.enabled)
        .on_click(move |checked: &bool, _, cx| {
          PluginManager::set_source_enabled(&toggle, *checked, cx)
        }),
    )
    .child(
      Button::new(format!("source-remove-{name}"))
        .icon(IconName::Delete)
        .cursor_pointer()
        .ghost()
        .danger()
        .disabled(name == OFFICIAL_SOURCE)
        .on_click(move |_, _, cx| {
          if let Err(e) = PluginManager::remove_source(&remove, cx) {
            tracing::error!("removing plugin source `{remove}`: {e:#}");
          }
        }),
    )
}

/// The kind picked for a new source
struct NewKind(SourceKind);

fn add_source(window: &mut Window, cx: &mut App) -> Div {
  let name = window.use_keyed_state("plugins-source-name", cx, |window, cx| {
    InputState::new(window, cx).placeholder(t!("app.settings.plugins.add.name"))
  });
  let location = window.use_keyed_state("plugins-source-location", cx, |window, cx| {
    InputState::new(window, cx).placeholder(t!("app.settings.plugins.add.location"))
  });
  let kind: Entity<NewKind> =
    window.use_keyed_state("plugins-source-kind", cx, |_, _| NewKind(SourceKind::Git));
  let current = kind.read(cx).0;
  let kinds = [SourceKind::Git, SourceKind::Path, SourceKind::Dev]
    .into_iter()
    .map(|k| Choice {
      value: kind_name(k).to_string(),
      label: kind_label(k).into_owned(),
    })
    .collect();
  let pick = kind.clone();
  let select = searchable(
    "plugins-source-kind",
    kinds,
    Some(kind_name(current).to_string()),
    t!("app.settings.plugins.add.kind"),
    window,
    cx,
    move |value, cx| {
      let picked = match value.as_str() {
        "path" => SourceKind::Path,
        "dev" => SourceKind::Dev,
        _ => SourceKind::Git,
      };
      pick.update(cx, |kind, cx| {
        kind.0 = picked;
        cx.notify();
      });
    },
  );
  div()
    .flex()
    .items_center()
    .gap_2()
    .child(div().w(px(140.)).child(Input::new(&name)))
    .child(div().w(px(140.)).child(select))
    .child(div().flex_1().child(Input::new(&location)))
    .child(
      Button::new("plugins-source-add")
        .label(t!("app.settings.plugins.add.button"))
        .icon(IconName::Plus)
        .cursor_pointer()
        .on_click(move |_, window, cx| {
          let source = SourceConfig {
            name: name.read(cx).value().trim().to_string(),
            kind: kind.read(cx).0,
            location: location.read(cx).value().trim().to_string(),
            enabled: true,
          };
          match PluginManager::add_source(source, cx) {
            Ok(()) => {
              name.update(cx, |input, cx| input.set_value("", window, cx));
              location.update(cx, |input, cx| input.set_value("", window, cx));
            }
            Err(e) => tracing::error!("adding a plugin source: {e:#}"),
          }
        }),
    )
}

fn kind_name(kind: SourceKind) -> &'static str {
  match kind {
    SourceKind::Git => "git",
    SourceKind::Path => "path",
    SourceKind::Dev => "dev",
  }
}

fn plugins_group(cx: &App) -> SettingGroup {
  let rows = match cx.try_global::<PluginManager>() {
    Some(_) => PluginManager::list(cx),
    None => Vec::new(),
  };
  let mut items: Vec<SettingItem> = rows
    .into_iter()
    .map(|row| SettingItem::render(move |_, window, cx| plugin_row(&row, window, cx)))
    .collect();
  if items.is_empty() {
    items.push(SettingItem::render(|_, _, cx| {
      muted(t!("app.settings.plugins.none"), cx)
    }));
  }
  items.push(SettingItem::render(|_, _, _| {
    div().flex().justify_end().gap_2().child(
      Button::new("plugins-refresh")
        .label(t!("app.settings.plugins.refresh"))
        .icon(IconName::RefreshCw)
        .cursor_pointer()
        .outline()
        .on_click(|_, _, cx| PluginManager::refresh_catalogs(cx)),
    )
  }));
  SettingGroup::new()
    .title(t!("app.settings.plugins.groups.plugins"))
    .items(items)
}

/// Whether a plugin's README is shown
struct ReadmeOpen(bool);

fn plugin_row(row: &PluginStatus, window: &mut Window, cx: &mut App) -> Div {
  let id = row.entry.id.clone();
  let manager = cx.global::<PluginManager>();
  let enabling = manager.is_enabling(&id);
  let error = manager.plugin_error(&id).map(str::to_string);
  let icon = row
    .entry
    .icon
    .as_ref()
    .and_then(|file| PluginManager::asset(row, file, cx));
  let readme_open: Entity<ReadmeOpen> =
    window.use_keyed_state(format!("plugin-readme-{id}"), cx, |_, _| ReadmeOpen(false));
  let readme = match (&row.entry.readme, readme_open.read(cx).0) {
    (Some(file), true) => {
      PluginManager::asset(row, file, cx).and_then(|path| std::fs::read_to_string(path).ok())
    }
    _ => None,
  };

  let mut title = div()
    .flex()
    .items_center()
    .gap_2()
    .child(div().text_sm().child(row.entry.name.clone()))
    .child(muted(row.entry.version.clone(), cx))
    .child(
      Tag::new()
        .small()
        .with_variant(match row.kind {
          SourceKind::Dev => TagVariant::Warning,
          _ => TagVariant::Secondary,
        })
        .child(row.source.clone()),
    );
  if row.update_available {
    title = title.child(
      Tag::new()
        .small()
        .with_variant(TagVariant::Info)
        .child(t!("app.settings.plugins.update_available")),
    );
  }
  if row.kind == SourceKind::Dev && !row.shadows.is_empty() {
    title = title.child(
      Tag::new()
        .small()
        .with_variant(TagVariant::Warning)
        .child(t!("app.settings.plugins.dev_override")),
    );
  }

  let details = div()
    .flex()
    .flex_col()
    .flex_1()
    .min_w_0()
    .child(title)
    .when_some(row.entry.description.clone(), |d, text| {
      d.child(muted(text, cx))
    })
    .when(!row.shadows.is_empty(), |d| {
      d.child(muted(
        t!(
          "app.settings.plugins.shadows",
          sources = row.shadows.join(", ")
        ),
        cx,
      ))
    })
    .children(error_text(error.as_deref(), cx));

  let (enable_id, remove_id) = (id.clone(), id.clone());
  let header = div()
    .flex()
    .items_center()
    .gap_2()
    .child(match icon {
      Some(path) => img(path).size(px(32.)).rounded_md().into_any_element(),
      None => Icon::new(IconName::LayoutDashboard)
        .size(px(32.))
        .into_any_element(),
    })
    .child(details)
    .when(row.entry.readme.is_some(), |d| {
      let toggle = readme_open.clone();
      d.child(
        Button::new(format!("plugin-readme-{id}"))
          .icon(IconName::Info)
          .cursor_pointer()
          .ghost()
          .on_click(move |_, _, cx| {
            toggle.update(cx, |open, cx| {
              open.0 = !open.0;
              cx.notify();
            })
          }),
      )
    })
    .when(row.kind == SourceKind::Git && row.installed, |d| {
      d.child(
        Button::new(format!("plugin-remove-{id}"))
          .icon(IconName::Delete)
          .cursor_pointer()
          .ghost()
          .danger()
          .on_click(move |_, _, cx| PluginManager::remove(&remove_id, cx)),
      )
    })
    .child(match enabling {
      true => Spinner::new().into_any_element(),
      false => Switch::new(format!("plugin-enabled-{id}"))
        .checked(row.enabled)
        .on_click(move |checked: &bool, _, cx| match checked {
          true => PluginManager::enable(&enable_id, cx),
          false => PluginManager::disable(&enable_id, cx),
        })
        .into_any_element(),
    });

  div()
    .flex()
    .flex_col()
    .gap_2()
    .child(header)
    .when_some(readme, |d, text| {
      d.child(
        div()
          .p_2()
          .rounded_md()
          .bg(cx.theme().secondary)
          .child(TextView::markdown(format!("plugin-readme-text-{id}"), text)),
      )
    })
}

fn settings_group(manifest: &PluginManifest, cx: &App) -> SettingGroup {
  let items: Vec<SettingItem> = manifest
    .settings
    .iter()
    .map(|setting| setting_item(&manifest.id, setting, cx))
    .collect();
  SettingGroup::new()
    .title(manifest.name.clone())
    .items(items)
}

/// The value of `setting` of plugin `id`: the configured one when it fits,
/// else the default; so `Config::default()` gives the default
fn value(id: &str, setting: &Setting) -> impl Fn(&Config) -> Value + Clone + 'static {
  let (id, setting) = (id.to_string(), setting.clone());
  move |c: &Config| {
    c.plugin_settings
      .get(&id)
      .and_then(|values| values.get(&setting.key))
      .filter(|v| setting.accepts(v))
      .cloned()
      .unwrap_or_else(|| setting.default_value())
  }
}

fn put(id: &str, key: &str) -> impl Fn(&mut Config, Value) + Clone + 'static {
  let (id, key) = (id.to_string(), key.to_string());
  move |c: &mut Config, value: Value| {
    c.plugin_settings
      .entry(id.clone())
      .or_default()
      .insert(key.clone(), value);
  }
}

fn setting_item(id: &str, setting: &Setting, cx: &App) -> SettingItem {
  let get = value(id, setting);
  let set = put(id, &setting.key);
  let key = format!("plugin.{id}.{}", setting.key);
  let label = setting.label.clone();
  let item = match &setting.kind {
    SettingKind::Toggle { .. } => SettingItem::new(
      label,
      switch(
        move |c| get(c).as_bool().unwrap_or_default(),
        move |c, v| set(c, Value::Bool(v)),
      ),
    ),
    SettingKind::Text { .. } => SettingItem::new(
      label,
      text(
        key,
        move |c| get(c).as_str().unwrap_or_default().to_string(),
        move |c, v| set(c, Value::String(v)),
      ),
    ),
    SettingKind::Number { min, max, step, .. } => SettingItem::new(
      label,
      number(
        key,
        (
          min.unwrap_or(f64::MIN),
          max.unwrap_or(f64::MAX),
          step.unwrap_or(1.),
        ),
        move |c| get(c).as_f64().unwrap_or_default(),
        move |c, v| set(c, json_number(v)),
      ),
    ),
    SettingKind::Slider { min, max, step, .. } => SettingItem::new(
      label,
      slider(
        key,
        (*min as f32, *max as f32, *step as f32),
        move |c| get(c).as_f64().unwrap_or_default() as f32,
        move |c, v| set(c, json_number(f64::from(v))),
      ),
    ),
    SettingKind::Select { .. } => {
      let options: Vec<(SharedString, SharedString)> = DynamicOptions::of(id, setting, cx)
        .iter()
        .map(|o| (o.value.clone().into(), o.label.clone().into()))
        .collect();
      let default: SharedString = get(&Config::default())
        .as_str()
        .unwrap_or_default()
        .to_string()
        .into();
      let read = get.clone();
      SettingItem::new(
        label,
        SettingField::dropdown(
          options,
          move |cx| {
            read(cx.config())
              .as_str()
              .unwrap_or_default()
              .to_string()
              .into()
          },
          move |v: SharedString, cx| save(cx, |c| set(c, Value::String(v.to_string()))),
        )
        .default_value(default),
      )
    }
    SettingKind::List { .. } => {
      let (id, setting) = (id.to_string(), setting.clone());
      SettingItem::new(
        label,
        SettingField::render(move |_, window, cx| list_field(&id, &setting, window, cx)),
      )
    }
    SettingKind::Secret { placeholder } => {
      let (id, key, placeholder) = (id.to_string(), setting.key.clone(), placeholder.clone());
      SettingItem::new(
        label,
        SettingField::render(move |_, window, cx| {
          secret_field(&id, &key, placeholder.clone(), window, cx)
        }),
      )
    }
  };
  match &setting.description {
    Some(description) => item.description(description.clone()),
    None => item,
  }
}

/// A list of strings, one input per row, rows added and removed
fn list_field(id: &str, setting: &Setting, window: &mut Window, cx: &mut App) -> Div {
  let get = value(id, setting);
  let items: Vec<String> = get(cx.config())
    .as_array()
    .map(|items| {
      items
        .iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
    })
    .unwrap_or_default();
  let write = {
    let set = put(id, &setting.key);
    move |items: Vec<String>, cx: &mut App| {
      let value = Value::Array(items.into_iter().map(Value::String).collect());
      save(cx, |c| set(c, value));
    }
  };
  let prefix = format!("plugin.{id}.{}", setting.key);

  let rows: Vec<Div> = items
    .iter()
    .enumerate()
    .map(|(i, item)| {
      let input = window.use_keyed_state(format!("{prefix}.{i}"), cx, |window, cx| {
        InputState::new(window, cx).default_value(item.clone())
      });
      if input.read(cx).value() != item.as_str()
        && !input.read(cx).focus_handle(cx).is_focused(window)
      {
        input.update(cx, |input, cx| input.set_value(item.clone(), window, cx));
      }
      let (save_items, remove_items) = (items.clone(), items.clone());
      let (save_write, remove_write) = (write.clone(), write.clone());
      let saved = input.clone();
      div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().flex_1().child(Input::new(&input)))
        .child(
          Button::new(format!("{prefix}.{i}.save"))
            .icon(IconName::Check)
            .cursor_pointer()
            .ghost()
            .on_click(move |_, _, cx| {
              let mut items = save_items.clone();
              items[i] = saved.read(cx).value().to_string();
              save_write(items, cx);
            }),
        )
        .child(
          Button::new(format!("{prefix}.{i}.remove"))
            .icon(IconName::Close)
            .cursor_pointer()
            .ghost()
            .on_click(move |_, _, cx| {
              let mut items = remove_items.clone();
              items.remove(i);
              remove_write(items, cx);
            }),
        )
    })
    .collect();

  div()
    .flex()
    .flex_col()
    .gap_1()
    .w(px(320.))
    .children(rows)
    .child(
      div().child(
        Button::new(format!("{prefix}.add"))
          .label(t!("app.settings.plugins.list_add"))
          .icon(IconName::Plus)
          .cursor_pointer()
          .outline()
          .on_click(move |_, _, cx| {
            let mut items = items.clone();
            items.push(String::new());
            write(items, cx);
          }),
      ),
    )
}

/// A masked input for a secret setting and how storing it went. What is
/// stored is never read back.
struct SecretInput {
  input: Entity<InputState>,
  /// `Ok` once stored from here, the error when that failed
  status: Option<Result<(), String>>,
  _enter: Subscription,
}

impl SecretInput {
  /// Stores (`Some`) or removes (`None`) the secret, showing how it went
  fn write(&mut self, id: String, key: String, value: Option<String>, cx: &mut Context<Self>) {
    cx.spawn(async move |this, cx| {
      let result = match value {
        Some(value) => secrets::store(&id, &key, value).await.map(|()| true),
        None => secrets::remove(&id, &key).await.map(|()| false),
      };
      if result.is_ok() {
        cx.update(|cx| ScriptManager::secret_changed(&id, &key, cx));
      }
      this
        .update(cx, |this, cx| {
          this.status = match result {
            Ok(true) => Some(Ok(())),
            Ok(false) => None,
            Err(e) => Some(Err(format!("{e:#}"))),
          };
          cx.notify();
        })
        .ok();
    })
    .detach();
  }
}

fn secret_field(
  id: &str,
  key: &str,
  placeholder: Option<String>,
  window: &mut Window,
  cx: &mut App,
) -> Div {
  let prefix = format!("plugin.{id}.{key}");
  let (store_id, store_key) = (id.to_string(), key.to_string());
  let field = window.use_keyed_state(prefix.clone(), cx, |window, cx| {
    let input = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder(placeholder.unwrap_or_default())
    });
    let _enter = cx.subscribe_in(
      &input,
      window,
      move |field: &mut SecretInput, input, event: &InputEvent, window, cx| {
        if !matches!(event, InputEvent::PressEnter { .. }) {
          return;
        }
        let value = input.read(cx).value().to_string();
        if value.is_empty() {
          return;
        }
        input.update(cx, |input, cx| input.set_value("", window, cx));
        field.write(store_id.clone(), store_key.clone(), Some(value), cx);
      },
    );
    SecretInput {
      input,
      status: None,
      _enter,
    }
  });
  let (input, status) = {
    let field = field.read(cx);
    (field.input.clone(), field.status.clone())
  };
  let (clear_id, clear_key) = (id.to_string(), key.to_string());
  div()
    .flex()
    .flex_col()
    .gap_1()
    .w(px(320.))
    .child(
      div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().flex_1().child(Input::new(&input)))
        .child(
          Button::new(format!("{prefix}.clear"))
            .icon(IconName::Delete)
            .cursor_pointer()
            .ghost()
            .on_click(move |_, _, cx| {
              field.update(cx, |field, cx| {
                field.write(clear_id.clone(), clear_key.clone(), None, cx)
              })
            }),
        ),
    )
    .when(matches!(status, Some(Ok(()))), |d| {
      d.child(muted(t!("app.settings.plugins.secret_stored"), cx))
    })
    .children(error_text(status.and_then(Result::err).as_deref(), cx))
}

#[cfg(test)]
mod tests {
  use corona_script::plugin::settings::SelectOption;
  use serde_json::json;

  use super::*;

  fn setting(kind: SettingKind) -> Setting {
    Setting {
      key: "k".into(),
      label: "K".into(),
      description: None,
      kind,
    }
  }

  #[test]
  fn values_fall_back_to_the_default() {
    let setting = setting(SettingKind::Select {
      dynamic: false,
      default: "a".into(),
      options: vec![
        SelectOption {
          value: "a".into(),
          label: "A".into(),
        },
        SelectOption {
          value: "b".into(),
          label: "B".into(),
        },
      ],
    });
    let get = value("p", &setting);
    let set = put("p", "k");
    let mut config = Config::default();
    assert_eq!(get(&config), json!("a"));
    set(&mut config, json!("b"));
    assert_eq!(get(&config), json!("b"));
    assert_eq!(config.plugin_settings["p"]["k"], json!("b"));
    // not one of the options
    set(&mut config, json!("c"));
    assert_eq!(get(&config), json!("a"));
    // another plugin's value is not this one's
    assert_eq!(value("q", &setting)(&config), json!("a"));
  }
}
