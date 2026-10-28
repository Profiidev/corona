//! The option forms of the shell's bar widgets

use corona_components::settings_fields::{Choice, searchable};
use corona_config::{ConfigProvider, bar::BarConfig};
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{current_options, form, form_input, form_key, form_row, options, set_options},
};
use gpui_kit::{
  App, IntoElement, ParentElement, Styled, Window,
  assets::IconName,
  component::{
    Sizable,
    button::{Button, ButtonVariants},
  },
  div,
};
use rust_i18n::t;

use crate::widgets::{
  clock::Clock,
  control_center::{
    disks::{self, DisksButton},
    printers::{self, PrintersButton},
  },
  privacy::{self, Privacy},
  resource::{self, Resource},
  tray::{self, Tray},
};

pub(super) fn register(cx: &mut App) {
  let form = |form| WidgetOptions { form, label: None };
  cx.settings_mut()
    .widget_options::<Clock>(form(clock_form))
    .widget_options::<Resource>(WidgetOptions {
      form: resource_form,
      // several sit side by side, what they show tells them apart
      label: Some(|bar, slot| {
        let options: resource::Options = options(bar, slot);
        resource::stat_name(options.stat).into()
      }),
    })
    .widget_options::<Tray>(form(tray_form))
    .widget_options::<Privacy>(form(privacy_form))
    .widget_options::<PrintersButton>(form(printers_form))
    .widget_options::<DisksButton>(form(disks_form));
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
      t!("app.settings.bar.form.format"),
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
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default(),
      label: resource::stat_name(*stat).into(),
    })
    .collect();
  let selected = serde_json::to_value(current.stat)
    .ok()
    .and_then(|v| v.as_str().map(str::to_string));
  let stat_name = name.to_string();
  let stat = searchable(
    form_key(name, slot, "stat"),
    choices,
    selected,
    t!("app.settings.bar.form.shows"),
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
    .child(form_row(t!("app.settings.bar.form.shows"), stat))
    .child(form_row(t!("app.settings.bar.form.warning_at"), warning))
    .child(form_row(t!("app.settings.bar.form.critical_at"), critical))
    .child(form_row(t!("app.settings.bar.form.mount"), mount))
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
          Button::new(form_key(name, slot, &format!("blacklist-remove-{i}")))
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
        .child(t!("app.settings.bar.form.hidden_items")),
    )
    .children(rows)
    .child(
      div().child(
        Button::new(form_key(name, slot, "blacklist-add"))
          .label(t!("app.settings.bar.form.add_pattern"))
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
      t!("app.settings.bar.form.hide_idle"),
      gpui_kit::component::switch::Switch::new(form_key(&name, slot, "idle"))
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

fn printers_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  let current: printers::Options = options(bar, slot);
  let name = name.to_string();
  form()
    .child(form_row(
      t!("app.settings.bar.form.hide_no_jobs"),
      gpui_kit::component::switch::Switch::new(form_key(&name, slot, "idle"))
        .checked(current.hide_when_idle)
        .small()
        .on_click(move |checked, _, cx| {
          set_options(
            cx,
            &name,
            slot,
            printers::Options {
              hide_when_idle: *checked,
            },
          );
        }),
    ))
    .into_any_element()
}

fn disks_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  let current: disks::Options = options(bar, slot);
  let name = name.to_string();
  form()
    .child(form_row(
      t!("app.settings.bar.form.hide_no_disks"),
      gpui_kit::component::switch::Switch::new(form_key(&name, slot, "idle"))
        .checked(current.hide_when_idle)
        .small()
        .on_click(move |checked, _, cx| {
          set_options(
            cx,
            &name,
            slot,
            disks::Options {
              hide_when_idle: *checked,
            },
          );
        }),
    ))
    .into_any_element()
}
