//! A notification when the system battery runs low, another when it is almost
//! empty. Each comes once per discharge and goes away on charging.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use corona_config::{BatteryWarningConfig, ConfigProvider, observe_section};
use corona_notifications::{Notifications, Notify, Urgency};
use corona_power::{Battery, BatteryState, Power, PowerExt};
use gpui_kit::App;
use rust_i18n::t;

use crate::control_center::power::battery::duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
  Low,
  Critical,
}

#[derive(Debug, PartialEq)]
enum Step {
  Keep,
  /// Charging or warnings off: the warning goes, the next discharge warns again
  Clear,
  Warn(Level),
}

/// What to do with the battery as it is now, `warned` the level already shown
/// this discharge
fn step(warned: Option<Level>, battery: Option<&Battery>, config: &BatteryWarningConfig) -> Step {
  let Some(battery) = battery.filter(|b| {
    config.enabled && matches!(b.state, BatteryState::Discharging | BatteryState::Empty)
  }) else {
    return match warned {
      Some(_) => Step::Clear,
      None => Step::Keep,
    };
  };
  let level = match battery.percentage {
    p if p <= f64::from(config.critical_percent) => Some(Level::Critical),
    p if p <= f64::from(config.low_percent) => Some(Level::Low),
    _ => None,
  };
  // a percentage bouncing back up while discharging does not warn again
  match level {
    Some(level) if Some(level) > warned => Step::Warn(level),
    _ => Step::Keep,
  }
}

#[derive(Default)]
struct Warned {
  level: Option<Level>,
  /// The notification showing it, once the daemon answered
  id: Option<u32>,
}

pub fn init(cx: &mut App) {
  let (Some(power), Some(_)) = (
    cx.try_global::<Power>().cloned(),
    cx.try_global::<Notifications>(),
  ) else {
    return;
  };
  let warned: Rc<RefCell<Warned>> = Rc::default();
  {
    let warned = warned.clone();
    cx.observe(&power.battery, move |_, cx| check(&warned, cx))
      .detach();
  }
  observe_section(
    cx,
    |c| &c.notification.battery,
    move |_, cx| check(&warned, cx),
  );
}

fn check(warned: &Rc<RefCell<Warned>>, cx: &mut App) {
  let battery = cx.power().battery(cx);
  let step = step(
    warned.borrow().level,
    battery,
    &cx.config().notification.battery,
  );
  let level = match step {
    Step::Keep => return,
    Step::Clear => {
      *warned.borrow_mut() = Warned::default();
      None
    }
    Step::Warn(level) => {
      warned.borrow_mut().level = Some(level);
      Some(level)
    }
  };
  let notifications = cx.global::<Notifications>().clone();
  if let Some(id) = warned.borrow_mut().id.take() {
    let close = notifications.close(id);
    cx.spawn(async move |_| {
      if let Err(e) = close.await {
        tracing::warn!("battery notification: {e:#}");
      }
    })
    .detach();
  }
  let (Some(level), Some(battery)) = (level, battery) else {
    return;
  };

  let percent = t!(
    "app.power.warning.percent",
    percent = format!("{:.0}", battery.percentage)
  );
  let body = match battery.time_to_empty {
    Some(d) => format!(
      "{percent} · {}",
      t!("app.power.remaining", duration = duration(d))
    ),
    None => percent.into(),
  };
  let (summary, app_icon, urgency, timeout) = match level {
    Level::Low => (
      t!("app.power.warning.low"),
      "battery-low",
      Urgency::Normal,
      -1,
    ),
    Level::Critical => (
      t!("app.power.warning.critical"),
      "battery-caution",
      Urgency::Critical,
      0,
    ),
  };
  let send = notifications.send_notify(Notify {
    app_name: corona_config::APP_NAME.into(),
    app_icon: app_icon.into(),
    summary: summary.into(),
    body,
    actions: Vec::new(),
    urgency,
    hints: HashMap::new(),
    timeout,
  });
  let warned = warned.clone();
  cx.spawn(async move |_| match send.await {
    // ponytail: charging before the daemon answers leaves this one up, close it then if it shows
    Ok(id) => warned.borrow_mut().id = Some(id),
    Err(e) => tracing::warn!("battery notification: {e:#}"),
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use super::*;

  fn battery(percentage: f64, state: BatteryState) -> Battery {
    Battery {
      percentage,
      state,
      time_to_empty: None,
      time_to_full: None,
      energy_rate: 0.,
      energy: 0.,
      energy_full: 0.,
      energy_full_design: 0.,
      capacity: 100.,
      charge_threshold: None,
    }
  }

  #[test]
  fn warns_once_per_level() {
    let config = BatteryWarningConfig::default();
    let at = |warned, p| {
      step(
        warned,
        Some(&battery(p, BatteryState::Discharging)),
        &config,
      )
    };
    assert_eq!(at(None, 50.), Step::Keep);
    assert_eq!(at(None, 20.), Step::Warn(Level::Low));
    assert_eq!(at(Some(Level::Low), 12.), Step::Keep);
    assert_eq!(at(Some(Level::Low), 5.), Step::Warn(Level::Critical));
    assert_eq!(at(Some(Level::Critical), 3.), Step::Keep);
    // straight to critical, and a bounce back up keeps the warning
    assert_eq!(at(None, 4.), Step::Warn(Level::Critical));
    assert_eq!(at(Some(Level::Critical), 30.), Step::Keep);
  }

  #[test]
  fn clears_on_charging_or_off() {
    let config = BatteryWarningConfig::default();
    let charging = battery(4., BatteryState::Charging);
    assert_eq!(
      step(Some(Level::Critical), Some(&charging), &config),
      Step::Clear
    );
    assert_eq!(step(None, Some(&charging), &config), Step::Keep);
    assert_eq!(step(Some(Level::Low), None, &config), Step::Clear);
    let off = BatteryWarningConfig {
      enabled: false,
      ..config
    };
    let low = battery(4., BatteryState::Discharging);
    assert_eq!(step(Some(Level::Low), Some(&low), &off), Step::Clear);
    assert_eq!(step(None, Some(&low), &off), Step::Keep);
  }
}
