//! The launcher page: behavior, every command's alias and visibility, quicklinks,
//! snippets and where results come from

use std::rc::Rc;

use corona_config::{
  Config, ConfigProvider,
  launcher::{CommandConfig, Quicklink, SearchEngine, Snippet},
};
use corona_launcher::registry::Registry;
use gpui_kit::{
  App, AppContext, Div, Entity, Focusable, ParentElement, SharedString, Styled, Subscription,
  Window,
  assets::IconName,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
  },
  div, px,
};
use rust_i18n::t;

use crate::settings::{
  fields::{Choice, number, optional_text, save, searchable, switch, text},
  pages::{group, item, page},
};

pub(super) fn page_of(cx: &App) -> SettingPage {
  page(
    t!("app.settings.launcher.title"),
    IconName::Search,
    vec![
      behavior(cx),
      commands(cx),
      group(
        t!("app.settings.launcher.groups.quicklinks"),
        vec![records_item(
          "quicklinks",
          t!("app.settings.launcher.quicklinks.title"),
          t!("app.settings.launcher.quicklinks.description"),
          &[
            t!("app.settings.launcher.fields.name"),
            t!("app.settings.launcher.fields.url"),
            t!("app.settings.launcher.fields.icon"),
          ],
          |c| {
            c.launcher
              .quicklinks
              .iter()
              .map(|q| {
                vec![
                  q.name.clone(),
                  q.url.clone(),
                  q.icon.clone().unwrap_or_default(),
                ]
              })
              .collect()
          },
          |c, rows| {
            c.launcher.quicklinks = rows
              .into_iter()
              .map(|r| Quicklink {
                name: r[0].clone(),
                url: r[1].clone(),
                icon: some(&r[2]),
              })
              .collect()
          },
        )],
      ),
      group(
        t!("app.settings.launcher.groups.snippets"),
        vec![records_item(
          "snippets",
          t!("app.settings.launcher.snippets.title"),
          t!("app.settings.launcher.snippets.description"),
          &[
            t!("app.settings.launcher.fields.name"),
            t!("app.settings.launcher.fields.keyword"),
            t!("app.settings.launcher.fields.text"),
          ],
          |c| {
            c.launcher
              .snippets
              .iter()
              .map(|s| {
                vec![
                  s.name.clone(),
                  s.keyword.clone().unwrap_or_default(),
                  s.text.clone(),
                ]
              })
              .collect()
          },
          |c, rows| {
            c.launcher.snippets = rows
              .into_iter()
              .map(|r| Snippet {
                name: r[0].clone(),
                keyword: some(&r[1]),
                text: r[2].clone(),
              })
              .collect()
          },
        )],
      ),
      sources(),
    ],
  )
}

fn some(value: &str) -> Option<String> {
  (!value.trim().is_empty()).then(|| value.to_string())
}

fn behavior(cx: &App) -> SettingGroup {
  let engines: Vec<Choice> = cx
    .config()
    .launcher
    .search_engines
    .iter()
    .map(|e| Choice {
      value: e.keyword.clone(),
      label: format!("{} ({})", e.name, e.keyword),
    })
    .collect();
  group(
    t!("app.settings.launcher.groups.behavior"),
    vec![
      item(
        t!("app.settings.launcher.close_on_unfocus.title"),
        t!("app.settings.launcher.close_on_unfocus.description"),
        switch(
          |c| c.launcher.close_on_unfocus,
          |c, v| c.launcher.close_on_unfocus = v,
        ),
      ),
      item(
        t!("app.settings.launcher.preview.title"),
        t!("app.settings.launcher.preview.description"),
        switch(|c| c.launcher.preview, |c, v| c.launcher.preview = v),
      ),
      item(
        t!("app.settings.launcher.terminal.title"),
        t!("app.settings.launcher.terminal.description"),
        text(
          "launcher.terminal",
          |c| c.launcher.terminal.clone(),
          |c, v| c.launcher.terminal = v,
        ),
      ),
      item(
        t!("app.settings.launcher.section_limit.title"),
        t!("app.settings.launcher.section_limit.description"),
        number(
          "launcher.section_limit",
          (1., 20., 1.),
          |c| c.launcher.section_limit as f64,
          |c, v| c.launcher.section_limit = v as usize,
        ),
      ),
      SettingItem::new(
        t!("app.settings.launcher.search_engine.title"),
        SettingField::render(move |_, window, cx| {
          let selected = Some(cx.config().launcher.search_engine.clone());
          searchable(
            "launcher.search_engine",
            engines.clone(),
            selected,
            t!("app.settings.launcher.search_engine.title"),
            window,
            cx,
            |keyword, cx| save(cx, |c| c.launcher.search_engine = keyword),
          )
        }),
      )
      .description(t!("app.settings.launcher.search_engine.description").to_string()),
      list_item(
        "launcher.fallbacks",
        t!("app.settings.launcher.fallbacks.title"),
        t!("app.settings.launcher.fallbacks.description"),
        |c| c.launcher.fallbacks.clone(),
        |c, v| c.launcher.fallbacks = v,
      ),
      item(
        t!("app.settings.launcher.currency_refresh.title"),
        t!("app.settings.launcher.currency_refresh.description"),
        number(
          "launcher.currency_refresh_hours",
          (1., 168., 1.),
          |c| c.launcher.currency_refresh_hours as f64,
          |c, v| c.launcher.currency_refresh_hours = v as u32,
        ),
      ),
    ],
  )
}

/// Every command with its alias, favorite, hidden and enabled switches
fn commands(cx: &App) -> SettingGroup {
  let mut all: Vec<(SharedString, SharedString)> = cx
    .try_global::<Registry>()
    .map(|r| {
      r.all(cx)
        .iter()
        .map(|c| (c.id(), c.meta(cx).title))
        .collect()
    })
    .unwrap_or_default();
  all.sort_by(|a, b| a.1.cmp(&b.1));
  all.dedup_by(|a, b| a.0 == b.0);
  group(
    t!("app.settings.launcher.groups.commands"),
    all
      .into_iter()
      .map(|(id, title)| {
        let key = id.to_string();
        SettingItem::new(
          title,
          SettingField::render(move |_, window, cx| command_row(&key, window, cx)),
        )
        .description(id.to_string())
      })
      .collect(),
  )
}

fn command_entry<'c>(c: &'c mut Config, id: &str) -> &'c mut CommandConfig {
  c.launcher.commands.entry(id.to_string()).or_default()
}

/// Drops entries back at their defaults, so the settings file keeps only changes
fn tidy(c: &mut Config) {
  c.launcher
    .commands
    .retain(|_, entry| *entry != CommandConfig::default());
}

fn command_row(id: &str, window: &mut Window, cx: &mut App) -> Div {
  let entry = cx
    .config()
    .launcher
    .command(id)
    .cloned()
    .unwrap_or_default();
  let alias = keyed_input(
    format!("launcher-alias-{id}"),
    t!("app.settings.launcher.command.alias").into(),
    entry.alias.clone().unwrap_or_default(),
    {
      let id = id.to_string();
      move |value, cx| {
        save(cx, |c| {
          command_entry(c, &id).alias = some(&value);
          tidy(c);
        })
      }
    },
    window,
    cx,
  );
  let toggle =
    |name: &'static str, label: SharedString, on: bool, set: fn(&mut CommandConfig, bool)| {
      let id = id.to_string();
      Checkbox::new(SharedString::from(format!("launcher-{name}-{id}")))
        .checked(on)
        .label(label)
        .small()
        .on_click(move |checked: &bool, _, cx| {
          let checked = *checked;
          save(cx, |c| {
            set(command_entry(c, &id), checked);
            tidy(c);
          })
        })
    };
  div()
    .flex()
    .items_center()
    .gap_3()
    .child(div().w(px(110.)).child(Input::new(&alias).small()))
    .child(toggle(
      "favorite",
      t!("app.settings.launcher.command.favorite").into(),
      entry.favorite,
      |e, v| e.favorite = v,
    ))
    .child(toggle(
      "hidden",
      t!("app.settings.launcher.command.hidden").into(),
      entry.hidden,
      |e, v| e.hidden = v,
    ))
    .child(toggle(
      "enabled",
      t!("app.settings.launcher.command.enabled").into(),
      entry.enabled,
      |e, v| e.enabled = v,
    ))
}

fn sources() -> SettingGroup {
  group(
    t!("app.settings.launcher.groups.sources"),
    vec![
      list_item(
        "launcher.files.roots",
        t!("app.settings.launcher.files_roots.title"),
        t!("app.settings.launcher.files_roots.description"),
        |c| c.launcher.files.roots.clone(),
        |c, v| c.launcher.files.roots = v,
      ),
      list_item(
        "launcher.files.excludes",
        t!("app.settings.launcher.files_excludes.title"),
        t!("app.settings.launcher.files_excludes.description"),
        |c| c.launcher.files.excludes.clone(),
        |c, v| c.launcher.files.excludes = v,
      ),
      item(
        t!("app.settings.launcher.files_hidden.title"),
        t!("app.settings.launcher.files_hidden.description"),
        switch(
          |c| c.launcher.files.hidden,
          |c, v| c.launcher.files.hidden = v,
        ),
      ),
      item(
        t!("app.settings.launcher.files_ignore.title"),
        t!("app.settings.launcher.files_ignore.description"),
        switch(
          |c| c.launcher.files.respect_ignore,
          |c, v| c.launcher.files.respect_ignore = v,
        ),
      ),
      item(
        t!("app.settings.launcher.clipboard.title"),
        t!("app.settings.launcher.clipboard.description"),
        switch(
          |c| c.launcher.clipboard.enabled,
          |c, v| c.launcher.clipboard.enabled = v,
        ),
      ),
      item(
        t!("app.settings.launcher.clipboard_images.title"),
        t!("app.settings.launcher.clipboard_images.description"),
        switch(
          |c| c.launcher.clipboard.images,
          |c, v| c.launcher.clipboard.images = v,
        ),
      ),
      item(
        t!("app.settings.launcher.clipboard_image_size.title"),
        t!("app.settings.launcher.clipboard_image_size.description"),
        number(
          "launcher.clipboard.image_max_mb",
          (1., 100., 1.),
          |c| c.launcher.clipboard.image_max_mb.into(),
          |c, v| c.launcher.clipboard.image_max_mb = v as f32,
        ),
      ),
      item(
        t!("app.settings.launcher.clipboard_retention.title"),
        t!("app.settings.launcher.clipboard_retention.description"),
        number(
          "launcher.clipboard.image_retention_days",
          (1., 365., 1.),
          |c| c.launcher.clipboard.image_retention_days as f64,
          |c, v| c.launcher.clipboard.image_retention_days = v as u32,
        ),
      ),
      item(
        t!("app.settings.launcher.deepl_key.title"),
        t!("app.settings.launcher.deepl_key.description"),
        optional_text(
          "launcher.translate.deepl_key",
          |c| c.launcher.translate.deepl_key.clone(),
          |c, v| c.launcher.translate.deepl_key = v,
          |_| String::new(),
        ),
      ),
      item(
        t!("app.settings.launcher.deepl_key_file.title"),
        t!("app.settings.launcher.deepl_key_file.description"),
        optional_text(
          "launcher.translate.deepl_key_file",
          |c| {
            c.launcher
              .translate
              .deepl_key_file
              .as_ref()
              .map(|p| p.display().to_string())
          },
          |c, v| c.launcher.translate.deepl_key_file = v.map(Into::into),
          |_| String::new(),
        ),
      ),
      item(
        t!("app.settings.launcher.translate_target.title"),
        t!("app.settings.launcher.translate_target.description"),
        text(
          "launcher.translate.target",
          |c| c.launcher.translate.target.clone(),
          |c, v| c.launcher.translate.target = v,
        ),
      ),
      list_item(
        "launcher.close_all_exclude",
        t!("app.settings.launcher.close_all_exclude.title"),
        t!("app.settings.launcher.close_all_exclude.description"),
        |c| c.launcher.close_all_exclude.clone(),
        |c, v| c.launcher.close_all_exclude = v,
      ),
      records_item(
        "search_engines",
        t!("app.settings.launcher.search_engines.title"),
        t!("app.settings.launcher.search_engines.description"),
        &[
          t!("app.settings.launcher.fields.keyword"),
          t!("app.settings.launcher.fields.name"),
          t!("app.settings.launcher.fields.url"),
        ],
        |c| {
          c.launcher
            .search_engines
            .iter()
            .map(|e| vec![e.keyword.clone(), e.name.clone(), e.url.clone()])
            .collect()
        },
        |c, rows| {
          c.launcher.search_engines = rows
            .into_iter()
            .map(|r| SearchEngine {
              keyword: r[0].clone(),
              name: r[1].clone(),
              url: r[2].clone(),
            })
            .collect()
        },
      ),
    ],
  )
}

/// A list of strings, one field per row
fn list_item(
  key: &'static str,
  title: impl Into<SharedString>,
  description: impl Into<String>,
  get: fn(&Config) -> Vec<String>,
  set: fn(&mut Config, Vec<String>),
) -> SettingItem {
  records_item(
    key,
    title,
    description,
    &[std::borrow::Cow::Borrowed("")],
    move |c| get(c).into_iter().map(|v| vec![v]).collect(),
    move |c, rows| set(c, rows.into_iter().map(|mut r| r.remove(0)).collect()),
  )
}

/// Rows of several text fields, added and removed, each change saving the whole list
fn records_item(
  key: &'static str,
  title: impl Into<SharedString>,
  description: impl Into<String>,
  labels: &[std::borrow::Cow<'static, str>],
  get: impl Fn(&Config) -> Vec<Vec<String>> + Clone + 'static,
  set: impl Fn(&mut Config, Vec<Vec<String>>) + Clone + 'static,
) -> SettingItem {
  let labels: Vec<SharedString> = labels.iter().map(|l| l.to_string().into()).collect();
  let reset = {
    let set = set.clone();
    let get = get.clone();
    move |_: &mut Window, cx: &mut App| {
      let defaults = get(&Config::default());
      save(cx, |c| set(c, defaults))
    }
  };
  let changed = {
    let get = get.clone();
    move |cx: &App| get(cx.config()) != get(&Config::default())
  };
  SettingItem::new(
    title,
    SettingField::render(move |_, window, cx| {
      records(key, &labels, get.clone(), set.clone(), window, cx)
    })
    .on_reset(changed, reset),
  )
  .description(description.into())
}

fn records(
  key: &str,
  labels: &[SharedString],
  get: impl Fn(&Config) -> Vec<Vec<String>> + Clone + 'static,
  set: impl Fn(&mut Config, Vec<Vec<String>>) + Clone + 'static,
  window: &mut Window,
  cx: &mut App,
) -> Div {
  let rows = get(cx.config());
  let width = labels.len();
  let write: Rc<dyn Fn(Vec<Vec<String>>, &mut App)> =
    Rc::new(move |rows, cx| save(cx, |c| set(c, rows)));
  let lines: Vec<Div> = rows
    .iter()
    .enumerate()
    .map(|(i, row)| {
      let fields = (0..width).map(|f| {
        let (rows, write) = (rows.clone(), write.clone());
        let input = keyed_input(
          format!("launcher-{key}-{i}-{f}"),
          labels[f].clone(),
          row.get(f).cloned().unwrap_or_default(),
          move |value, cx| {
            let mut rows = rows.clone();
            if rows[i][f] != value {
              rows[i][f] = value;
              write(rows, cx);
            }
          },
          window,
          cx,
        );
        div().flex_1().min_w_0().child(Input::new(&input).small())
      });
      let (rows, write) = (rows.clone(), write.clone());
      div().flex().items_center().gap_1().children(fields).child(
        Button::new(SharedString::from(format!("launcher-{key}-{i}-remove")))
          .icon(IconName::Close)
          .ghost()
          .small()
          .on_click(move |_, _, cx| {
            let mut rows = rows.clone();
            rows.remove(i);
            write(rows, cx);
          }),
      )
    })
    .collect();
  let theme = cx.theme();
  div()
    .flex()
    .flex_col()
    .gap_1()
    .w(px(if width > 1 { 460. } else { 320. }))
    .when_labels(width > 1, labels, theme.colors.muted_foreground)
    .children(lines)
    .child(
      div().child(
        Button::new(SharedString::from(format!("launcher-{key}-add")))
          .label(t!("app.settings.plugins.list_add"))
          .icon(IconName::Plus)
          .outline()
          .small()
          .on_click(move |_, _, cx| {
            let mut rows = rows.clone();
            rows.push(vec![String::new(); width]);
            write(rows, cx);
          }),
      ),
    )
}

trait LabelsExt {
  fn when_labels(self, show: bool, labels: &[SharedString], color: gpui_kit::Hsla) -> Self;
}

impl LabelsExt for Div {
  fn when_labels(self, show: bool, labels: &[SharedString], color: gpui_kit::Hsla) -> Self {
    if !show {
      return self;
    }
    self.child(
      div()
        .flex()
        .gap_1()
        .pr(px(28.))
        .text_size(px(12.))
        .text_color(color)
        .children(labels.iter().map(|l| div().flex_1().child(l.clone()))),
    )
  }
}

struct Keyed {
  input: Entity<InputState>,
  last: String,
  _subscription: Subscription,
}

/// A field kept across renders under `key`, committing on Enter or blur and following
/// changes made elsewhere while it is not focused
fn keyed_input(
  key: String,
  placeholder: SharedString,
  value: String,
  commit: impl Fn(String, &mut App) + 'static,
  window: &mut Window,
  cx: &mut App,
) -> Entity<InputState> {
  let commit = Rc::new(commit);
  let shown = value.clone();
  let field = window.use_keyed_state(key, cx, move |window, cx| {
    let input = cx.new(|cx| {
      InputState::new(window, cx)
        .placeholder(placeholder)
        .default_value(shown.clone())
    });
    let commit = commit.clone();
    let _subscription = cx.subscribe(
      &input,
      move |field: &mut Keyed, input, e: &InputEvent, cx| {
        if matches!(e, InputEvent::PressEnter { .. } | InputEvent::Blur) {
          let value = input.read(cx).value().to_string();
          if value != field.last {
            field.last = value.clone();
            commit(value, cx);
          }
        }
      },
    );
    Keyed {
      input,
      last: shown,
      _subscription,
    }
  });
  let input = field.read(cx).input.clone();
  if value != field.read(cx).last && !input.read(cx).focus_handle(cx).is_focused(window) {
    field.update(cx, |field, cx| {
      field.last = value.clone();
      field
        .input
        .update(cx, |input, cx| input.set_value(value, window, cx));
    });
  }
  input
}
