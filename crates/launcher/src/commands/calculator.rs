//! The calculator row: math, percent, units, currency, bases, time zones, dates and
//! unix timestamps, plus its history and exchange rates

use std::{
  collections::HashMap,
  rc::Rc,
  sync::{Arc, RwLock},
  time::{Duration, Instant},
};

use anyhow::{Context as _, Result};
use corona_config::ConfigProvider;
use futures::AsyncReadExt;
use gpui_kit::{App, Global, SharedString, assets::IconName, http_client::AsyncBody};
use jiff::{Span, Timestamp, Zoned, civil::Time, tz::TimeZone};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::{
  command::{Action, Clip, Command, CommandSource, Evaluation, Icon, Meta, Outcome, SourceKind},
  kit::ListSource,
  matching::Query,
  registry, simple, state,
  view::ListView,
};

const RATES_URL: &str = "https://api.frankfurter.app/latest?from=EUR";
const BUDGET: Duration = Duration::from_millis(40);
const HISTORY: usize = 200;

/// EUR based rates: one euro in each currency
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Rates {
  date: String,
  /// Unix seconds of the fetch
  fetched: u64,
  rates: HashMap<String, f64>,
}

#[derive(Default)]
struct Calculator {
  rates: Arc<RwLock<Rates>>,
  fetching: bool,
}

impl Global for Calculator {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Entry {
  expression: String,
  result: String,
  time: u64,
}

pub fn init(cx: &mut App) {
  let rates: Rates = state::load("rates");
  cx.set_global(Calculator {
    rates: Arc::new(RwLock::new(rates)),
    fetching: false,
  });
  refresh_rates(false, cx);
  registry::register_source(Rc::new(Evaluator), cx);
  registry::register_all([history(), refresh()], cx);
}

/// Fetches the rates when they are older than the config allows, or always when forced
fn refresh_rates(force: bool, cx: &mut App) -> Option<gpui_kit::Task<Result<()>>> {
  let max_age = u64::from(cx.config().launcher.currency_refresh_hours.max(1)) * 3600;
  let calculator = cx.global::<Calculator>();
  let fetched = calculator.rates.read().map(|r| r.fetched).unwrap_or(0);
  if calculator.fetching || (!force && state::now().saturating_sub(fetched) < max_age) {
    return None;
  }
  cx.global_mut::<Calculator>().fetching = true;
  let client = cx.http_client();
  let rates = cx.global::<Calculator>().rates.clone();
  Some(cx.spawn(async move |cx| {
    let result = async {
      let mut response = client
        .get(RATES_URL, AsyncBody::empty(), true)
        .await
        .context("fetching exchange rates")?;
      let mut body = Vec::new();
      response.body_mut().read_to_end(&mut body).await?;
      let mut fresh: Rates = serde_json::from_slice(&body).context("reading exchange rates")?;
      fresh.rates.insert("EUR".into(), 1.);
      fresh.fetched = state::now();
      let _ = state::save("rates", &fresh);
      *rates.write().map_err(|_| anyhow::anyhow!("rates lock"))? = fresh;
      Ok(())
    }
    .await;
    cx.update(|cx| cx.global_mut::<Calculator>().fetching = false);
    if let Err(e) = &result {
      tracing::warn!("exchange rates: {e:#}");
    }
    result
  }))
}

struct RateHandler(Arc<RwLock<Rates>>);

impl fend_core::ExchangeRateFnV2 for RateHandler {
  fn relative_to_base_currency(
    &self,
    currency: &str,
    _: &fend_core::ExchangeRateFnV2Options,
  ) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let rates = self.0.read().map_err(|_| "rates lock")?;
    rates
      .rates
      .get(currency)
      .copied()
      .ok_or_else(|| format!("no rate for {currency}").into())
  }
}

struct Deadline(Instant);

impl fend_core::Interrupt for Deadline {
  fn should_interrupt(&self) -> bool {
    Instant::now() > self.0
  }
}

/// Spellings fend does not know: currency symbols, `F`/`C` temperatures, `dec`/`bin`,
/// adding and taking percentages
pub fn normalize(input: &str) -> String {
  let symbols = [("€", "EUR"), ("$", "USD"), ("£", "GBP"), ("¥", "JPY")];
  // `€30` is `30 EUR`
  let input: Vec<String> = input
    .split_whitespace()
    .map(|w| {
      symbols
        .iter()
        .find_map(|(s, code)| {
          let n = w.strip_prefix(s)?;
          n.parse::<f64>().is_ok().then(|| format!("{n} {code}"))
        })
        .unwrap_or_else(|| w.to_string())
    })
    .collect();
  let mut text = format!(" {} ", input.join(" "));
  for (from, to) in [
    ("€", " EUR "),
    ("$", " USD "),
    ("£", " GBP "),
    ("¥", " JPY "),
    (" yen ", " JPY "),
    (" euros ", " EUR "),
    (" euro ", " EUR "),
    (" dollars ", " USD "),
    (" dec ", " decimal "),
    (" bin ", " binary "),
  ] {
    text = text.replace(from, to);
  }
  let words: Vec<&str> = text.split_whitespace().collect();
  let mut out: Vec<String> = Vec::new();
  for (i, w) in words.iter().enumerate() {
    // `100 F` and `in C` as temperatures; farad and coulomb are rare in a launcher
    let temperature = matches!(*w, "F" | "C")
      && (i > 0 && words[i - 1].parse::<f64>().is_ok()
        || i > 0 && matches!(words[i - 1], "to" | "in" | "as"));
    out.push(match (temperature, *w) {
      (true, "F") => "°F".into(),
      (true, "C") => "°C".into(),
      _ => w.to_string(),
    });
  }
  let text = out.join(" ");
  // `80 + 15%` is 92, not 80.15
  if let Some((left, right)) = text.rsplit_once(['+', '-'])
    && let Some(percent) = right.trim().strip_suffix('%')
    && percent.trim().parse::<f64>().is_ok()
    && !left.trim().is_empty()
  {
    let sign = if text[left.len()..].starts_with('+') {
      '+'
    } else {
      '-'
    };
    return format!("({}) * (1 {sign} {} / 100)", left.trim(), percent.trim());
  }
  text
}

/// The result of fend for `input`, None when it is not a calculation
pub fn evaluate(input: &str, rates: Option<Arc<RwLock<Rates>>>) -> Option<String> {
  if !input.chars().any(|c| c.is_ascii_digit()) && !input.contains("pi") {
    return None;
  }
  let mut context = fend_core::Context::new();
  if let Some(rates) = rates {
    context.set_exchange_rate_handler_v2(RateHandler(rates));
  }
  let normalized = normalize(input);
  let result = fend_core::evaluate_with_interrupt(
    &normalized,
    &mut context,
    &Deadline(Instant::now() + BUDGET),
  )
  .ok()?;
  let result = result.get_main_result().trim();
  let result = match result.strip_prefix("approx. ") {
    Some(rest) => format!("≈ {rest}"),
    None => result.to_string(),
  };
  let same = |a: &str, b: &str| {
    let squash = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    squash(a).eq_ignore_ascii_case(&squash(b))
  };
  (!result.is_empty() && !same(&result, input) && !same(&result, &normalized)).then_some(result)
}

/// A place in the time zone database: `tokyo`, `new york`, `Europe/Berlin`, `utc`
fn zone(place: &str) -> Option<TimeZone> {
  let wanted = place.trim().replace(' ', "_").to_lowercase();
  if wanted.is_empty() {
    return None;
  }
  if matches!(wanted.as_str(), "utc" | "gmt" | "z") {
    return Some(TimeZone::UTC);
  }
  let db = jiff::tz::db();
  let name = db.available().find(|name| {
    let lower = name.as_str().to_lowercase();
    lower == wanted || lower.rsplit('/').next() == Some(wanted.as_str())
  })?;
  db.get(name.as_str()).ok()
}

/// `3pm`, `15:30`, `9am`
fn clock(text: &str) -> Option<Time> {
  let text = text.trim().to_lowercase();
  let (text, offset) = match (text.strip_suffix("pm"), text.strip_suffix("am")) {
    (Some(t), _) => (t.trim().to_string(), 12),
    (_, Some(t)) => (t.trim().to_string(), 0),
    _ => (text, -1),
  };
  let (h, m) = match text.split_once(':') {
    Some((h, m)) => (h.parse::<i8>().ok()?, m.parse::<i8>().ok()?),
    None => (text.parse::<i8>().ok()?, 0),
  };
  let h = match offset {
    12 if h < 12 => h + 12,
    0 if h == 12 => 0,
    -1 if text.len() <= 2 && !text.contains(':') => return None,
    _ => h,
  };
  Time::new(h, m, 0, 0).ok()
}

fn show(zoned: &Zoned) -> String {
  zoned.strftime("%a %d %b %Y, %H:%M %Z").to_string()
}

/// Time zones, dates and timestamps, which fend does not do
pub fn evaluate_time(input: &str, now: &Zoned) -> Option<String> {
  let lower = input.trim().to_lowercase();
  // unix timestamps both ways
  if lower == "now to unix" || lower == "now in unix" || lower == "unix" {
    return Some(now.timestamp().as_second().to_string());
  }
  if lower.len() >= 9 && lower.len() <= 13 && lower.chars().all(|c| c.is_ascii_digit()) {
    let n: i64 = lower.parse().ok()?;
    let ts = match lower.len() {
      12 | 13 => Timestamp::from_millisecond(n).ok()?,
      _ => Timestamp::from_second(n).ok()?,
    };
    return Some(show(&ts.to_zoned(now.time_zone().clone())));
  }
  // `now in tokyo`, `time in new york`
  for prefix in ["now in ", "time in ", "now at "] {
    if let Some(place) = lower.strip_prefix(prefix) {
      let tz = zone(place)?;
      return Some(show(&now.with_time_zone(tz)));
    }
  }
  // `3pm berlin to london`
  if let Some((from, to)) = lower
    .split_once(" to ")
    .or_else(|| lower.split_once(" in "))
  {
    let (time, place) = from.split_once(' ')?;
    let time = clock(time)?;
    let source = zone(place)?;
    let target = zone(to)?;
    let at = now.with_time_zone(source).with().time(time).build().ok()?;
    return Some(show(&at.with_time_zone(target)));
  }
  // `today + 90 days`, `tomorrow`, `today - 2 weeks`
  let (base, rest) = match lower.split_once(['+', '-']) {
    Some((base, rest)) => (
      base.trim(),
      Some((lower[base.len()..].starts_with('+'), rest.trim())),
    ),
    None => (lower.as_str(), None),
  };
  let day = match base {
    "today" | "now" => now.date(),
    "tomorrow" => now.date().tomorrow().ok()?,
    "yesterday" => now.date().yesterday().ok()?,
    _ => return None,
  };
  let date = match rest {
    None if base == "now" => return None,
    None => day,
    Some((plus, span)) => {
      let span: Span = span.parse().ok()?;
      let span = if plus { span } else { span.negate() };
      day.checked_add(span).ok()?
    }
  };
  Some(date.strftime("%A, %d %B %Y").to_string())
}

/// What the calculator row would show for `input`
pub fn calculate(input: &str, cx: &App) -> Option<String> {
  evaluate_time(input, &Zoned::now())
    .or_else(|| evaluate(input, Some(cx.global::<Calculator>().rates.clone())))
}

fn rates_note(cx: &App) -> Option<SharedString> {
  let calculator = cx.global::<Calculator>();
  let rates = calculator.rates.read().ok()?;
  if rates.fetched == 0 {
    return None;
  }
  let hours = state::now().saturating_sub(rates.fetched) / 3600;
  Some(
    t!(
      "app.launcher.calculator.rates_age",
      date = rates.date.clone(),
      hours = hours
    )
    .into(),
  )
}

fn is_currency(input: &str) -> bool {
  let upper = normalize(input).to_uppercase();
  [
    "EUR", "USD", "GBP", "JPY", "CHF", "CAD", "AUD", "CNY", "SEK", "NOK", "DKK", "PLN", "CZK",
  ]
  .iter()
  .any(|c| upper.split_whitespace().any(|w| w == *c))
}

fn record(expression: &str, result: &str) {
  let mut history: Vec<Entry> = state::load("calc_history");
  history.retain(|e| e.expression != expression);
  history.insert(
    0,
    Entry {
      expression: expression.to_string(),
      result: result.to_string(),
      time: state::now(),
    },
  );
  history.truncate(HISTORY);
  let _ = state::save("calc_history", &history);
}

fn result_command(
  expression: String,
  result: String,
  note: Option<SharedString>,
  loading: bool,
) -> Rc<dyn Command> {
  let (expr, res) = (expression.clone(), result.clone());
  simple(
    "calculator",
    move |_| Meta {
      title: res.clone().into(),
      subtitle: Some(expr.clone().into()),
      icon: Icon::Tile(IconName::Calculator, 30.),
      evaluation: Some(Evaluation {
        input: expr.clone().into(),
        output: match loading {
          true => t!("app.launcher.calculator.fetching").into(),
          false => res.clone().into(),
        },
        note: note.clone(),
      }),
      ..Default::default()
    },
    move |_| {
      let (e1, r1, e2, r2) = (
        expression.clone(),
        result.clone(),
        expression.clone(),
        result.clone(),
      );
      vec![
        Action::sync(
          "copy",
          t!("app.launcher.calculator.copy"),
          move |_, _, _| {
            record(&e1, &r1);
            Ok(Outcome::Copy(Clip::Text(r1.clone())))
          },
        ),
        Action::sync(
          "copy_all",
          t!("app.launcher.calculator.copy_all"),
          move |_, _, _| {
            record(&e2, &r2);
            Ok(Outcome::Copy(Clip::Text(format!("{e2} = {r2}"))))
          },
        )
        .shortcut("ctrl-enter"),
      ]
    },
  )
}

struct Evaluator;

impl CommandSource for Evaluator {
  fn id(&self) -> SharedString {
    "calculator".into()
  }

  fn kind(&self) -> SourceKind {
    SourceKind::Evaluator
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let input = query.raw.trim().to_string();
    if input.is_empty() {
      return Vec::new();
    }
    let now = Zoned::now();
    if let Some(result) = evaluate_time(&input, &now) {
      return vec![result_command(input, result, None, false)];
    }
    let calculator = cx.global::<Calculator>();
    let currency = is_currency(&input);
    match evaluate(&input, Some(calculator.rates.clone())) {
      Some(result) => {
        let note = currency.then(|| rates_note(cx)).flatten();
        vec![result_command(input, result, note, false)]
      }
      None if currency && calculator.fetching => {
        vec![result_command(input, String::new(), rates_note(cx), true)]
      }
      None => Vec::new(),
    }
  }

  fn loading(&self, cx: &App) -> bool {
    cx.global::<Calculator>().fetching
  }
}

fn history() -> Rc<dyn Command> {
  crate::kit::opener(
    "calculator:history",
    t!("app.launcher.calculator.history"),
    Icon::Tile(IconName::FileClock, 30.),
    &["calculator", "calc", "math"],
    |_| {
      ListView::new(
        t!("app.launcher.calculator.history"),
        t!("app.launcher.calculator.search_history"),
        Rc::new(ListSource::new("calculator:history", |_, _| {
          let history: Vec<Entry> = state::load("calc_history");
          history
            .into_iter()
            .map(|e| {
              let (title, subtitle, copy) = (e.result.clone(), e.expression.clone(), e.result);
              simple(
                format!("calc:{}", e.expression),
                move |_| Meta {
                  title: title.clone().into(),
                  subtitle: Some(subtitle.clone().into()),
                  icon: Icon::Named(IconName::Calculator),
                  ..Default::default()
                },
                move |_| {
                  let copy = copy.clone();
                  vec![Action::sync(
                    "copy",
                    t!("app.launcher.calculator.copy"),
                    move |_, _, _| Ok(Outcome::Copy(Clip::Text(copy.clone()))),
                  )]
                },
              )
            })
            .collect()
        })),
      )
      .empty(t!("app.launcher.calculator.no_history"))
      .into_view()
    },
  )
}

fn refresh() -> Rc<dyn Command> {
  simple(
    "calculator:rates",
    |cx| Meta {
      title: t!("app.launcher.calculator.refresh_rates").into(),
      subtitle: rates_note(cx),
      icon: Icon::Tile(IconName::RefreshCw, 30.),
      keywords: vec!["currency".into(), "exchange".into()],
      ..Default::default()
    },
    |_| {
      vec![Action::spawn(
        "refresh",
        t!("app.launcher.calculator.refresh_rates"),
        |_, cx| {
          let task = refresh_rates(true, cx);
          async move {
            if let Some(task) = task {
              task.await?;
            }
            Ok(Outcome::hud(t!("app.launcher.calculator.rates_updated")))
          }
        },
      )]
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn calc(input: &str) -> Option<String> {
    let rates = Rates {
      date: "2026-10-23".into(),
      fetched: 1,
      rates: HashMap::from([("EUR".into(), 1.), ("USD".into(), 2.), ("JPY".into(), 160.)]),
    };
    evaluate(input, Some(Arc::new(RwLock::new(rates))))
  }

  #[test]
  fn math() {
    assert_eq!(calc("2^10 / 4").as_deref(), Some("256"));
    assert!(calc("sqrt(2)").unwrap().starts_with("≈ 1.414"));
    assert_eq!(calc("20% of 80").as_deref(), Some("16"));
    assert_eq!(calc("80 + 15%").as_deref(), Some("92"));
    assert_eq!(calc("80 - 25%").as_deref(), Some("60"));
  }

  #[test]
  fn units_and_bases() {
    assert!(calc("5 km to mi").unwrap().contains("3.10"));
    assert!(
      calc("100 F in C").unwrap().contains("37.7"),
      "{:?}",
      calc("100 F in C")
    );
    assert_eq!(calc("0xff to dec").as_deref(), Some("255"));
    assert!(calc("255 in bin").unwrap().contains("11111111"));
  }

  #[test]
  fn currency() {
    assert_eq!(calc("50 eur to usd").as_deref(), Some("100 USD"));
    assert_eq!(calc("€30 in yen").as_deref(), Some("4800 JPY"));
  }

  #[test]
  fn not_calculations() {
    for text in ["firefox", "vol 40", "5", "settings"] {
      assert_eq!(calc(text), None, "{text}");
    }
  }

  #[test]
  fn times() {
    let now: Zoned = "2026-10-23T12:00[Europe/Berlin]".parse().unwrap();
    let t = |s| evaluate_time(s, &now);
    assert_eq!(
      t("1735689600").as_deref(),
      Some("Wed 01 Jan 2025, 01:00 CET")
    );
    assert_eq!(t("now to unix").as_deref(), Some("1792749600"));
    assert!(t("now in tokyo").unwrap().contains("19:00"));
    assert!(t("3pm berlin to london").unwrap().contains("14:00"));
    assert_eq!(
      t("today + 90 days").as_deref(),
      Some("Thursday, 21 January 2027")
    );
    assert_eq!(t("tomorrow").as_deref(), Some("Saturday, 24 October 2026"));
    assert_eq!(t("firefox"), None);
    assert_eq!(t("3pm nowhere to london"), None);
  }
}
