//! The resource widget's options form

use corona_components::settings_fields::{Choice, number, searchable};
use corona_config::{ConfigProvider, bar::BarConfig};
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{current_options, form, form_input, form_key, form_row, options, set_options},
  item,
};
use gpui_kit::{App, IntoElement, ParentElement, Styled, Window, component::Sizable};
use rust_i18n::t;

use crate::bar::sysinfo::{self as resource, Resource};

pub(super) fn register(cx: &mut App) {
  cx.settings_mut()
    .group("system", "polling", 20, || {
      t!("app.settings.system.groups.polling").into()
    })
    .group_items("system", "polling", 0, |_| {
      vec![item(
        t!("app.settings.system.system_monitor.title"),
        t!("app.settings.system.system_monitor.description"),
        number(
          "system.monitor.poll_seconds",
          (1., 60., 1.),
          |c| c.system.monitor.poll_seconds as f64,
          |c, v| c.system.monitor.poll_seconds = v as u64,
        ),
      )]
    })
    .widget_options::<Resource>(WidgetOptions {
      form: resource_form,
      // several sit side by side, what they show tells them apart
      label: Some(|bar, slot| {
        let options: resource::Options = options(bar, slot);
        resource::stat_name(options.stat).into()
      }),
    });
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
          .filter(|v| v.is_finite())
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
