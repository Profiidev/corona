//! When the nightlight is on and how warm: by hand, or from sunset to sunrise or a schedule,
//! waking only at the next start or end and once a minute while fading.

use std::time::Duration;

use anyhow::{Result, bail};
use corona_config::{ConfigProvider, NightlightConfig, NightlightMode, observe_section};
use corona_location::{LocationExt, LocationService};
use gpui_kit::{App, AppContext, Entity};
use jiff::{SignedDuration, Timestamp, civil::Time, tz::TimeZone};

use crate::{
  gamma::{self, IDENTITY_KELVIN, MIN_KELVIN},
  solar::{self, Day},
};

/// Fades step this often
const STEP: SignedDuration = SignedDuration::from_secs(60);
/// Plans at least this often, so a polar night or summer is looked at again
const REPLAN: SignedDuration = SignedDuration::from_hours(24);
/// Stands in for "not in sight", far enough that no fade reaches now
const FAR: SignedDuration = SignedDuration::from_hours(24 * 10);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nightlight {
  /// The compositor lets the shell change gamma
  pub available: bool,
  pub on: bool,
  /// Kelvin at night, fades pass through warmer ones
  pub temperature: u32,
}

pub(crate) enum Change {
  On(bool),
  Temperature(u32),
  /// The `[nightlight]` settings changed: the temperature starts over from them
  Config,
  /// The location changed
  Replan,
}

pub(crate) fn clamp(kelvin: u32) -> u32 {
  kelvin.clamp(MIN_KELVIN, IDENTITY_KELVIN)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plan {
  pub on: bool,
  /// What the outputs show now
  pub kelvin: u32,
  /// When to look again, None: only on a change
  pub next: Option<Timestamp>,
  /// When the timed mode next turns on or off
  pub boundary: Option<Timestamp>,
}

fn parse_time(text: &str, default: &str) -> Time {
  text.parse().unwrap_or_else(|_| {
    tracing::warn!("nightlight: {text:?} is no HH:MM time, using {default}");
    default.parse().unwrap()
  })
}

/// The nights around `now` for the timed modes, oldest first. None: the mode is manual, or
/// solar without a known place.
pub(crate) fn nights(
  now: Timestamp,
  config: &NightlightConfig,
  coordinates: Option<(f64, f64)>,
  tz: &TimeZone,
) -> Option<Vec<(Timestamp, Timestamp)>> {
  match config.mode {
    NightlightMode::Manual => None,
    NightlightMode::Schedule => {
      let defaults = NightlightConfig::default();
      let start = parse_time(&config.start, &defaults.start);
      let end = parse_time(&config.end, &defaults.end);
      if start == end {
        return Some(Vec::new());
      }
      let today = now.to_zoned(tz.clone()).date();
      let at = |date: jiff::civil::Date, time| date.to_datetime(time).to_zoned(tz.clone());
      Some(
        (-2..=2)
          .filter_map(|offset| {
            let day = today.checked_add(jiff::Span::new().days(offset)).ok()?;
            let end_day = match end > start {
              true => day,
              false => day.tomorrow().ok()?,
            };
            Some((
              at(day, start).ok()?.timestamp(),
              at(end_day, end).ok()?.timestamp(),
            ))
          })
          .collect(),
      )
    }
    NightlightMode::Solar => {
      let (latitude, longitude) = coordinates?;
      let today = now.to_zoned(TimeZone::UTC).date();
      let suns: Vec<Day> = (-3..=3)
        .filter_map(|offset| today.checked_add(jiff::Span::new().days(offset)).ok())
        .map(|date| solar::sun(date, latitude, longitude))
        .collect();
      // every sunset to the sunrise after it
      let mut events: Vec<(Timestamp, bool)> = suns
        .iter()
        .flat_map(|sun| match *sun {
          Day::Rises { rise, set } => vec![(rise, true), (set, false)],
          _ => vec![],
        })
        .collect();
      events.sort();
      let Some(&(first, first_rises)) = events.first() else {
        // weeks without either
        return Some(match suns[3] {
          Day::PolarNight => vec![(now - FAR, now + FAR)],
          _ => Vec::new(),
        });
      };
      let mut nights = Vec::new();
      if first_rises {
        nights.push((first - FAR, first));
      }
      let mut dark = None;
      for (at, rises) in events {
        match (rises, dark) {
          (false, None) => dark = Some(at),
          (true, Some(set)) => {
            nights.push((set, at));
            dark = None;
          }
          _ => {}
        }
      }
      if let Some(set) = dark {
        nights.push((set, set + FAR));
      }
      Some(nights)
    }
  }
}

fn lerp(from: u32, to: u32, part: f64) -> u32 {
  (from as f64 + (to as f64 - from as f64) * part.clamp(0., 1.)).round() as u32
}

/// Where `now` falls among `nights`: fades over `transition` after each start and before each
/// end, never longer than half the night
pub(crate) fn plan(
  now: Timestamp,
  nights: &[(Timestamp, Timestamp)],
  transition: SignedDuration,
  kelvin: u32,
) -> Plan {
  let off = |next: Option<Timestamp>| Plan {
    on: false,
    kelvin: IDENTITY_KELVIN,
    next,
    boundary: next,
  };
  let Some(&(start, end)) = nights.iter().find(|(_, end)| *end > now) else {
    return off(Some(now + REPLAN));
  };
  if now < start {
    return off(Some(start.min(now + REPLAN)));
  }
  let fade = transition.min(end.duration_since(start) / 2);
  let (kelvin, next) = if now < start + fade {
    let part = now.duration_since(start).as_secs_f64() / fade.as_secs_f64();
    (
      lerp(IDENTITY_KELVIN, kelvin, part),
      (now + STEP).min(start + fade),
    )
  } else if now < end - fade {
    (kelvin, end - fade)
  } else if fade.is_positive() {
    let part = now.duration_since(end - fade).as_secs_f64() / fade.as_secs_f64();
    (lerp(kelvin, IDENTITY_KELVIN, part), (now + STEP).min(end))
  } else {
    (kelvin, end)
  };
  Plan {
    on: true,
    kelvin,
    next: Some(next.min(now + REPLAN)),
    boundary: Some(end),
  }
}

/// The hand set state
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Manual {
  /// The manual mode's switch
  pub on: bool,
  /// Overrides a timed mode until then
  pub until: Option<(bool, Timestamp)>,
}

pub(crate) fn decide(
  now: Timestamp,
  config: &NightlightConfig,
  nights: Option<&[(Timestamp, Timestamp)]>,
  kelvin: u32,
  manual: &mut Manual,
) -> Plan {
  let by_hand = |on: bool, next| Plan {
    on,
    kelvin: if on { kelvin } else { IDENTITY_KELVIN },
    next,
    boundary: next,
  };
  if config.mode == NightlightMode::Manual {
    return by_hand(manual.on, None);
  }
  let Some(nights) = nights else {
    // solar without a place
    return by_hand(false, None);
  };
  let transition = SignedDuration::from_mins(config.transition_minutes.into());
  let plan = plan(now, nights, transition, kelvin);
  match manual.until {
    Some((on, until)) if until > now => Plan {
      boundary: plan.boundary,
      ..by_hand(on, Some(until))
    },
    _ => {
      manual.until = None;
      plan
    }
  }
}

pub(crate) fn start(cx: &mut App) -> (Entity<Nightlight>, flume::Sender<Change>) {
  let temperature = clamp(cx.config().nightlight.temperature);
  let state = cx.new(|_| Nightlight {
    available: false,
    on: false,
    temperature,
  });
  let (gamma, commands) = flume::unbounded();
  let (available_tx, available) = flume::unbounded();
  gamma::spawn(commands, available_tx);
  cx.spawn({
    let state = state.clone();
    async move |cx| {
      if let Ok(available) = available.recv_async().await {
        state.update(cx, |state, cx| {
          state.available = available;
          cx.notify();
        });
      }
    }
  })
  .detach();

  let (changes_tx, changes) = flume::unbounded();
  observe_section(cx, |c| &c.nightlight, {
    let changes = changes_tx.clone();
    move |_, _| _ = changes.send(Change::Config)
  });
  if cx.has_global::<LocationService>() {
    let changes = changes_tx.clone();
    cx.observe(&cx.location().location.clone(), move |_, _| {
      _ = changes.send(Change::Replan)
    })
    .detach();
  }

  let entity = state.clone();
  cx.spawn(async move |cx| {
    let mut manual = Manual::default();
    let mut temperature = temperature;
    let mut sent = None;
    let mut warned = false;
    loop {
      let now = Timestamp::now();
      let plan = cx.update(|cx| {
        let config = cx.config().nightlight.clone();
        let coordinates = cx
          .has_global::<LocationService>()
          .then(|| cx.location().current(cx))
          .flatten()
          .map(|l| (l.latitude, l.longitude));
        if config.mode == NightlightMode::Solar && coordinates.is_none() && !warned {
          tracing::warn!("nightlight: sunset is unknown without a [location], staying off");
          warned = true;
        }
        let nights = nights(now, &config, coordinates, &TimeZone::system());
        decide(now, &config, nights.as_deref(), temperature, &mut manual)
      });
      let kelvin = plan.on.then_some(plan.kelvin);
      if sent != Some(kelvin) {
        let _ = gamma.send(kelvin);
        sent = Some(kelvin);
      }
      entity.update(cx, |state, cx| {
        if (state.on, state.temperature) != (plan.on, temperature) {
          (state.on, state.temperature) = (plan.on, temperature);
          cx.notify();
        }
      });

      let wait = plan
        .next
        .map(|next| Duration::try_from(next.duration_since(now)).unwrap_or_default());
      let change = futures_lite::future::or(
        async {
          match wait {
            Some(wait) => cx.background_executor().timer(wait).await,
            None => futures_lite::future::pending().await,
          }
          Some(Change::Replan)
        },
        async { changes.recv_async().await.ok() },
      )
      .await;
      match change {
        None => break,
        Some(Change::On(on)) => match cx.update(|cx| cx.config().nightlight.mode) {
          NightlightMode::Manual => manual.on = on,
          _ => {
            let until = plan.boundary.unwrap_or(Timestamp::now() + FAR);
            manual.until = Some((on, until));
          }
        },
        Some(Change::Temperature(kelvin)) => temperature = kelvin,
        Some(Change::Config) => {
          temperature = cx.update(|cx| clamp(cx.config().nightlight.temperature));
          manual.until = None;
          warned = false;
        }
        Some(Change::Replan) => warned = false,
      }
    }
  })
  .detach();
  (state, changes_tx)
}

pub(crate) fn send(
  state: &Entity<Nightlight>,
  changes: &flume::Sender<Change>,
  change: Change,
  cx: &App,
) -> Result<()> {
  if !state.read(cx).available {
    bail!("nightlight is unavailable: the compositor does not allow changing gamma");
  }
  changes.send(change)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use jiff::civil::date;

  use super::*;

  fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
  }

  fn schedule(start: &str, end: &str, minutes: u32) -> NightlightConfig {
    NightlightConfig {
      mode: NightlightMode::Schedule,
      start: start.into(),
      end: end.into(),
      transition_minutes: minutes,
      ..Default::default()
    }
  }

  fn run(now: &str, config: &NightlightConfig, manual: &mut Manual) -> Plan {
    let now = at(now);
    let nights = nights(now, config, Some((52.52, 13.405)), &TimeZone::UTC);
    decide(now, config, nights.as_deref(), 3000, manual)
  }

  #[test]
  fn manual_follows_the_switch() {
    let config = NightlightConfig::default();
    let mut manual = Manual::default();
    let off = run("2026-06-21T12:00:00Z", &config, &mut manual);
    assert_eq!(
      (off.on, off.kelvin, off.next),
      (false, IDENTITY_KELVIN, None)
    );
    manual.on = true;
    let on = run("2026-06-21T12:00:00Z", &config, &mut manual);
    assert_eq!((on.on, on.kelvin, on.next), (true, 3000, None));
  }

  #[test]
  fn schedule_over_midnight() {
    let config = schedule("22:00", "06:00", 0);
    let mut manual = Manual::default();
    let day = run("2026-06-21T12:00:00Z", &config, &mut manual);
    assert!(!day.on);
    assert_eq!(day.next, Some(at("2026-06-21T22:00:00Z")));
    let night = run("2026-06-21T23:00:00Z", &config, &mut manual);
    assert_eq!((night.on, night.kelvin), (true, 3000));
    assert_eq!(night.next, Some(at("2026-06-22T06:00:00Z")));
    // after midnight it is still the same night
    let early = run("2026-06-22T05:59:00Z", &config, &mut manual);
    assert!(early.on);
    assert!(!run("2026-06-22T06:00:00Z", &config, &mut manual).on);
    // a schedule within the day
    let config = schedule("01:00", "03:00", 0);
    assert!(run("2026-06-21T02:00:00Z", &config, &mut manual).on);
    assert!(!run("2026-06-21T04:00:00Z", &config, &mut manual).on);
    // start and end the same: never on
    let config = schedule("01:00", "01:00", 0);
    assert!(!run("2026-06-21T01:00:00Z", &config, &mut manual).on);
  }

  #[test]
  fn fades_step_each_minute() {
    let config = schedule("20:00", "07:00", 30);
    let mut manual = Manual::default();
    // halfway into the fade in
    let fading = run("2026-06-21T20:15:00Z", &config, &mut manual);
    assert_eq!(fading.kelvin, lerp(IDENTITY_KELVIN, 3000, 0.5));
    assert_eq!(fading.next, Some(at("2026-06-21T20:16:00Z")));
    // the last step lands on the end of the fade
    let last = run("2026-06-21T20:29:30Z", &config, &mut manual);
    assert_eq!(last.next, Some(at("2026-06-21T20:30:00Z")));
    let full = run("2026-06-21T20:30:00Z", &config, &mut manual);
    assert_eq!(full.kelvin, 3000);
    assert_eq!(full.next, Some(at("2026-06-22T06:30:00Z")));
    // fading out before the end
    let out = run("2026-06-22T06:45:00Z", &config, &mut manual);
    assert_eq!(out.kelvin, lerp(3000, IDENTITY_KELVIN, 0.5));
    assert!(out.on);
    // never longer than half the night
    let short = schedule("20:00", "20:10", 30);
    let middle = run("2026-06-21T20:05:00Z", &short, &mut manual);
    assert_eq!(middle.kelvin, 3000);
  }

  #[test]
  fn bad_times_fall_back() {
    let config = schedule("late", "25:00", 0);
    let mut manual = Manual::default();
    assert!(run("2026-06-21T21:00:00Z", &config, &mut manual).on);
    assert!(!run("2026-06-21T19:00:00Z", &config, &mut manual).on);
  }

  #[test]
  fn solar_follows_the_sun() {
    let config = NightlightConfig {
      mode: NightlightMode::Solar,
      transition_minutes: 0,
      ..Default::default()
    };
    let mut manual = Manual::default();
    // Berlin at the June solstice: dark from about 19:33 to 02:43 UTC
    assert!(!run("2026-06-21T12:00:00Z", &config, &mut manual).on);
    let night = run("2026-06-21T23:00:00Z", &config, &mut manual);
    assert!(night.on);
    let Day::Rises { rise, .. } = solar::sun(date(2026, 6, 22), 52.52, 13.405) else {
      panic!()
    };
    assert_eq!(night.next, Some(rise));

    // without a place it stays off
    let now = at("2026-06-21T23:00:00Z");
    assert_eq!(nights(now, &config, None, &TimeZone::UTC), None);
    assert!(!decide(now, &config, None, 3000, &mut manual).on);
  }

  #[test]
  fn polar_night_and_midnight_sun() {
    let config = NightlightConfig {
      mode: NightlightMode::Solar,
      ..Default::default()
    };
    let tromso = Some((69.65, 18.96));
    let mut manual = Manual::default();
    let now = at("2026-12-21T12:00:00Z");
    let nights = nights(now, &config, tromso, &TimeZone::UTC).unwrap();
    let dark = decide(now, &config, Some(&nights), 3000, &mut manual);
    assert_eq!((dark.on, dark.kelvin), (true, 3000));
    assert!(dark.next.unwrap() <= now + REPLAN);

    let now = at("2026-06-21T00:00:00Z");
    let nights = nights_for(now, &config, tromso);
    let light = decide(now, &config, Some(&nights), 3000, &mut manual);
    assert!(!light.on);
    assert_eq!(light.next, Some(now + REPLAN));
  }

  fn nights_for(
    now: Timestamp,
    config: &NightlightConfig,
    coordinates: Option<(f64, f64)>,
  ) -> Vec<(Timestamp, Timestamp)> {
    nights(now, config, coordinates, &TimeZone::UTC).unwrap()
  }

  #[test]
  fn overrides_last_until_the_next_boundary() {
    let config = schedule("22:00", "06:00", 0);
    let mut manual = Manual::default();
    let day = run("2026-06-21T12:00:00Z", &config, &mut manual);
    // turned on by hand during the day: on until the schedule starts
    manual.until = Some((true, day.boundary.unwrap()));
    let forced = run("2026-06-21T13:00:00Z", &config, &mut manual);
    assert_eq!((forced.on, forced.kelvin), (true, 3000));
    assert_eq!(forced.next, Some(at("2026-06-21T22:00:00Z")));
    // then the schedule takes over again
    let night = run("2026-06-21T22:00:00Z", &config, &mut manual);
    assert!(night.on);
    assert_eq!(manual.until, None);
    // off by hand at night: off until the morning
    manual.until = Some((false, night.boundary.unwrap()));
    assert!(!run("2026-06-22T01:00:00Z", &config, &mut manual).on);
    assert!(!run("2026-06-22T06:00:00Z", &config, &mut manual).on);
    assert!(run("2026-06-22T22:00:01Z", &config, &mut manual).on);
  }
}
