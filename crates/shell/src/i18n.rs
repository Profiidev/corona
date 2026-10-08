use std::sync::{LazyLock, RwLock};

use icu_calendar::{Date, Iso};
use icu_datetime::{
  DateTimeFormatter, NoCalendarFormatter, fieldsets,
  input::{DateTime, Time},
};
use icu_decimal::{DecimalFormatter, input::Decimal};
use icu_locale::Locale;
use jiff::{
  civil,
  fmt::{
    Write,
    strtime::{BrokenDownTime, Config, Custom, Extension},
  },
};
use jiff_icu::ConvertInto;

/// The `fallback` of `i18n!`, for messages, dates and numbers alike
const FALLBACK: &str = "en";

/// Languages with translations, from the locale files
pub fn languages() -> Vec<String> {
  rust_i18n::available_locales!()
    .into_iter()
    .map(|locale| locale.into_owned())
    .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Locales {
  /// One of [`languages`]
  pub messages: String,
  pub time: Locale,
  pub numeric: Locale,
}

impl Default for Locales {
  fn default() -> Self {
    Self {
      messages: FALLBACK.to_string(),
      time: icu_locale::locale!("en"),
      numeric: icu_locale::locale!("en"),
    }
  }
}

/// Layers our `gpui_component:` translations over the components' own. Call
/// once, before `gpui_kit` initializes.
pub fn extend_components() {
  use gpui_kit::component as gpui_component;
  rust_i18n::extend!(gpui_component);
}

static CURRENT: LazyLock<RwLock<Locales>> = LazyLock::new(Default::default);

/// The locales from the environment and `language`, made current
pub fn apply(language: Option<&str>) {
  let locales = resolve(language, |name| std::env::var(name).ok(), &languages());
  rust_i18n::set_locale(&locales.messages);
  *CURRENT.write().unwrap_or_else(|e| e.into_inner()) = locales;
}

fn current() -> Locales {
  CURRENT.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// The POSIX lookup per category: `LC_ALL`, then the category, then an explicit
/// `language`, then `LANG`. `C`, `POSIX` and empty values count as unset.
/// Messages take the closest of the `available` translations, `pt-BR` before `pt`.
pub fn resolve(
  language: Option<&str>,
  env: impl Fn(&str) -> Option<String>,
  available: &[impl AsRef<str>],
) -> Locales {
  let fallback = Locale::try_from_str(FALLBACK).unwrap_or(Locale::UNKNOWN);
  let lookup = |category: &str| {
    ["LC_ALL", category]
      .iter()
      .find_map(|name| env(name).and_then(|v| parse(&v)))
      .or(language.and_then(parse))
      .or_else(|| env("LANG").and_then(|v| parse(&v)))
      .unwrap_or(fallback.clone())
  };
  let messages = lookup("LC_MESSAGES");
  let (tag, language) = (messages.to_string(), messages.id.language.as_str());
  Locales {
    messages: available
      .iter()
      .map(AsRef::as_ref)
      .find(|a| *a == tag)
      .or_else(|| available.iter().map(AsRef::as_ref).find(|a| *a == language))
      .unwrap_or(FALLBACK)
      .to_string(),
    time: lookup("LC_TIME"),
    numeric: lookup("LC_NUMERIC"),
  }
}

/// `de_DE.UTF-8@euro` as `de-DE`
fn parse(value: &str) -> Option<Locale> {
  let tag = value.split(['.', '@']).next()?.replace('_', "-");
  match tag.as_str() {
    "" | "C" | "POSIX" => None,
    _ => Locale::try_from_str(&tag).ok(),
  }
}

/// `value` with `digits` decimals in the numeric locale, like `12,5` in German
pub fn decimal(value: f64, digits: usize) -> String {
  format_decimal(&current().numeric, value, digits)
}

fn format_decimal(locale: &Locale, value: f64, digits: usize) -> String {
  let plain = format!("{value:.digits$}");
  let (Ok(formatter), Ok(number)) = (
    DecimalFormatter::try_new(locale.into(), Default::default()),
    plain.parse::<Decimal>(),
  ) else {
    return plain;
  };
  formatter.format(&number).to_string()
}

/// `time` in the strftime `pattern`, names and `%c %x %X` in the time locale. A
/// pattern from the settings may be bad, then the error shows instead, where a
/// plain `strftime` would panic.
pub fn format_time(pattern: &str, time: impl Into<BrokenDownTime>) -> String {
  format_time_in(&current().time, pattern, time.into())
}

fn format_time_in(locale: &Locale, pattern: &str, tm: BrokenDownTime) -> String {
  let pattern = match tm.to_date() {
    Ok(date) => localize_names(locale, pattern, date),
    Err(_) => pattern.to_string(),
  };
  let config = Config::new().custom(IcuCustom(locale.clone()));
  tm.to_string_with_config(&config, &pattern)
    .unwrap_or_else(|e| format!("bad format: {e}"))
}

/// `date` formatted with one field set in `locale`, `None` when ICU has no data
macro_rules! names {
  ($locale:expr, $date:expr, $fields:expr) => {{
    let date: Date<Iso> = $date.convert_into();
    DateTimeFormatter::try_new($locale.into(), $fields)
      .ok()
      .map(|f| f.format(&date).to_string())
  }};
}

/// `pattern` with `%A %a %B %b %h` replaced by the names in `locale`, escaped
/// for strftime. A `^` flag uppercases the name; widths are dropped.
fn localize_names(locale: &Locale, pattern: &str, date: civil::Date) -> String {
  let mut out = String::with_capacity(pattern.len());
  let mut rest = pattern;
  while let Some(at) = rest.find('%') {
    out.push_str(&rest[..at]);
    let directive = &rest[at + 1..];
    let flags = directive
      .find(|c: char| !matches!(c, '-' | '_' | '0' | '^' | '#') && !c.is_ascii_digit())
      .unwrap_or(directive.len());
    let Some(spec) = directive[flags..].chars().next() else {
      out.push_str(&rest[at..]);
      return out;
    };
    let end = at + 1 + flags + spec.len_utf8();
    let name = match spec {
      'A' => names!(locale, date, fieldsets::E::long()),
      'a' => names!(locale, date, fieldsets::E::short()),
      'B' => names!(locale, date, fieldsets::M::long()),
      'b' | 'h' => names!(locale, date, fieldsets::M::medium()),
      _ => None,
    };
    match name {
      Some(mut name) => {
        if directive[..flags].contains('^') {
          name = name.to_uppercase();
        }
        out.push_str(&name.replace('%', "%%"));
      }
      _ => out.push_str(&rest[at..end]),
    }
    rest = &rest[end..];
  }
  out.push_str(rest);
  out
}

/// `%c %x %X` in the locale
struct IcuCustom(Locale);

impl IcuCustom {
  fn write<W: Write>(&self, text: Option<String>, wtr: &mut W) -> Result<(), jiff::Error> {
    let text = text.ok_or_else(|| jiff::Error::from_args(format_args!("not localizable")))?;
    wtr.write_str(&text)
  }
}

impl Custom for IcuCustom {
  fn format_datetime<W: Write>(
    &self,
    _config: &Config<Self>,
    _ext: &Extension,
    tm: &BrokenDownTime,
    wtr: &mut W,
  ) -> Result<(), jiff::Error> {
    let dt: DateTime<Iso> = tm.to_datetime()?.convert_into();
    let text =
      DateTimeFormatter::try_new((&self.0).into(), fieldsets::YMD::medium().with_time_hms())
        .ok()
        .map(|f| f.format(&dt).to_string());
    self.write(text, wtr)
  }

  fn format_date<W: Write>(
    &self,
    _config: &Config<Self>,
    _ext: &Extension,
    tm: &BrokenDownTime,
    wtr: &mut W,
  ) -> Result<(), jiff::Error> {
    let date: Date<Iso> = tm.to_date()?.convert_into();
    let text = DateTimeFormatter::try_new((&self.0).into(), fieldsets::YMD::medium())
      .ok()
      .map(|f| f.format(&date).to_string());
    self.write(text, wtr)
  }

  fn format_time<W: Write>(
    &self,
    _config: &Config<Self>,
    _ext: &Extension,
    tm: &BrokenDownTime,
    wtr: &mut W,
  ) -> Result<(), jiff::Error> {
    let time: Time = tm.to_time()?.convert_into();
    let text = NoCalendarFormatter::try_new((&self.0).into(), fieldsets::T::hms())
      .ok()
      .map(|f| f.format(&time).to_string());
    self.write(text, wtr)
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use icu_locale::{Locale, locale};
  use jiff::civil::date;

  use super::{FALLBACK, format_decimal, format_time_in};

  fn env(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let vars: HashMap<String, String> = vars
      .iter()
      .map(|(k, v)| (k.to_string(), v.to_string()))
      .collect();
    move |name| vars.get(name).cloned()
  }

  fn de() -> Locale {
    locale!("de-DE")
  }

  /// `resolve` with translations for these languages only
  fn resolve(language: Option<&str>, env: impl Fn(&str) -> Option<String>) -> super::Locales {
    super::resolve(language, env, &["en", "de", "pt", "pt-BR"])
  }

  #[test]
  fn resolves_like_posix() {
    let all_de = resolve(None, env(&[("LANG", "de_DE.UTF-8")]));
    assert_eq!(
      (all_de.messages.as_str(), all_de.time, all_de.numeric),
      ("de", de(), de())
    );

    let unset = resolve(None, env(&[]));
    assert_eq!(
      (unset.messages.as_str(), unset.time, unset.numeric),
      ("en", locale!("en"), locale!("en"))
    );

    let all = resolve(
      Some("en"),
      env(&[("LC_ALL", "de_DE.UTF-8"), ("LC_MESSAGES", "en_US.UTF-8")]),
    );
    assert_eq!((all.messages.as_str(), all.time), ("de", de()));

    let posix = resolve(None, env(&[("LANG", "C"), ("LC_ALL", "POSIX")]));
    assert_eq!((posix.messages.as_str(), posix.time), ("en", locale!("en")));

    // no French translation, French dates all the same
    let french = resolve(None, env(&[("LANG", "fr_FR.UTF-8")]));
    assert_eq!(
      (french.messages.as_str(), french.time),
      ("en", locale!("fr-FR"))
    );

    let explicit = resolve(Some("de"), env(&[("LANG", "en_US.UTF-8")]));
    assert_eq!(
      (explicit.messages.as_str(), explicit.time),
      ("de", locale!("de"))
    );
    let category = resolve(Some("de"), env(&[("LC_MESSAGES", "en_US.UTF-8")]));
    assert_eq!(category.messages, "en");
  }

  #[test]
  fn resolves_the_closest_translation() {
    let brazil = resolve(None, env(&[("LANG", "pt_BR.UTF-8")]));
    assert_eq!(brazil.messages, "pt-BR");
    let portugal = resolve(None, env(&[("LANG", "pt_PT.UTF-8")]));
    assert_eq!(portugal.messages, "pt");
    let unknown = resolve(Some("xx"), env(&[]));
    assert_eq!(unknown.messages, "en");
  }

  /// NixOS `i18n.defaultLocale = "en_US.UTF-8"` with German `extraLocaleSettings`
  #[test]
  fn resolves_mixed_categories() {
    let mixed = resolve(
      None,
      env(&[
        ("LANG", "en_US.UTF-8"),
        ("LC_TIME", "de_DE.UTF-8"),
        ("LC_NUMERIC", "de_DE.UTF-8"),
        ("LC_MEASUREMENT", "de_DE.UTF-8"),
      ]),
    );
    assert_eq!(
      (mixed.messages.as_str(), mixed.time, mixed.numeric),
      ("en", de(), de())
    );
  }

  #[test]
  fn decimals_follow_the_locale() {
    assert_eq!(format_decimal(&de(), 12.5, 1), "12,5");
    assert_eq!(format_decimal(&locale!("en-US"), 12.5, 1), "12.5");
    assert_eq!(format_decimal(&de(), 1234.56, 2), "1.234,56");
    assert_eq!(format_decimal(&locale!("en"), -0.25, 1), "-0.2");
  }

  #[test]
  fn names_follow_the_locale() {
    let at = date(2026, 10, 8).at(9, 5, 3, 0);
    let fmt = |locale: &Locale, pattern: &str| format_time_in(locale, pattern, at.into());

    assert_eq!(fmt(&de(), "%A, %-d. %B"), "Donnerstag, 8. Oktober");
    assert_eq!(fmt(&de(), "%a %b %h"), "Do Okt Okt");
    assert_eq!(fmt(&de(), "%^B"), "OKTOBER");
    assert_eq!(fmt(&de(), "%%A %H:%M"), "%A 09:05");
    assert_eq!(fmt(&de(), "%x"), "08.10.2026");
    assert_eq!(fmt(&de(), "%X"), "09:05:03");

    // the default clock pattern reads as it did before localization
    let en = locale!("en-US");
    assert_eq!(fmt(&en, "%H:%M %a, %b %-d"), "09:05 Thu, Oct 8");
    assert_eq!(fmt(&en, "%A %B"), "Thursday October");
  }

  #[test]
  fn bad_patterns_do_not_panic() {
    let at = date(2026, 10, 8).at(9, 5, 3, 0);
    assert!(format_time_in(&de(), "%Q", at.into()).starts_with("bad format"));
    assert!(format_time_in(&de(), "50%", at.into()).starts_with("bad format"));
  }

  /// Translations of `locale` below `prefix`, as key to text
  fn messages(locale: &str, prefix: &str) -> HashMap<String, String> {
    crate::_rust_i18n_backend()
      .messages_for_locale(locale)
      .unwrap_or_default()
      .into_iter()
      .filter(|(key, _)| key.starts_with(prefix))
      .map(|(key, text)| (key.into_owned(), text.into_owned()))
      .collect()
  }

  fn placeholders(text: &str) -> Vec<&str> {
    let mut found: Vec<_> = text
      .match_indices("%{")
      .filter_map(|(at, _)| Some(&text[at..at + text[at..].find('}')? + 1]))
      .collect();
    found.sort();
    found
  }

  #[test]
  fn locales_have_the_same_keys() {
    let locales = super::languages();
    assert!(locales.len() >= 2, "{locales:?}");

    let en = messages(FALLBACK, "app.");
    assert!(!en.is_empty());
    for locale in &locales {
      let other = messages(locale, "app.");
      let mut missing: Vec<_> = en.keys().filter(|k| !other.contains_key(*k)).collect();
      missing.extend(other.keys().filter(|k| !en.contains_key(*k)));
      missing.sort();
      assert!(missing.is_empty(), "{locale} differs from en: {missing:?}");
      for (key, text) in &en {
        assert_eq!(
          placeholders(text),
          placeholders(&other[key]),
          "placeholders differ for {key} in {locale}"
        );
      }
    }
  }

  /// Every key in the sources, in `t!("…")` or a table of keys, has a text in
  /// every locale, and every text is used
  #[test]
  fn used_keys_exist() {
    fn sources(dir: &std::path::Path, out: &mut Vec<String>) {
      for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
          sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
          out.push(std::fs::read_to_string(path).unwrap());
        }
      }
    }
    let mut files = Vec::new();
    sources(
      &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
      &mut files,
    );

    let call = regex::Regex::new(r#""(app\.[a-z0-9_.]+)""#).unwrap();
    let mut keys: Vec<String> = files
      .iter()
      .flat_map(|file| call.captures_iter(file).map(|c| c[1].to_string()))
      .collect();
    // keys built at runtime
    keys.extend(
      corona_weather::Condition::ALL
        .iter()
        .map(|c| format!("app.weather.condition.{}", c.key())),
    );
    keys.extend((0..8).map(|i| {
      format!(
        "app.weather.compass.{}",
        corona_weather::compass(f64::from(i) * 45.)
      )
    }));
    // the settings name built-in bar widgets by their type
    let widget =
      regex::Regex::new(r#"impl Widget for \w+ \{\s*const NAME: &'static str = "(\w+)""#).unwrap();
    let widgets: Vec<String> = files
      .iter()
      .flat_map(|file| widget.captures_iter(file).map(|c| c[1].to_string()))
      .collect();
    assert!(widgets.len() > 10, "found only {} widgets", widgets.len());
    keys.extend(
      widgets
        .iter()
        .map(|w| format!("app.settings.bar.widget.{w}")),
    );
    assert!(keys.len() > 300, "found only {} keys", keys.len());

    let mut unused: Vec<_> = messages(FALLBACK, "app.")
      .into_keys()
      .filter(|k| !keys.contains(k))
      .collect();
    unused.sort();
    assert!(unused.is_empty(), "never used: {unused:?}");

    for locale in super::languages() {
      let known = messages(&locale, "app.");
      let mut missing: Vec<_> = keys.iter().filter(|k| !known.contains_key(*k)).collect();
      missing.sort();
      missing.dedup();
      assert!(missing.is_empty(), "missing in {locale}: {missing:?}");
    }
  }

  #[test]
  fn interpolates() {
    let text = |locale: &str| {
      rust_i18n::t!("app.weather.rain_chance", locale = locale, percent = 40).into_owned()
    };
    assert_eq!(text("en"), "40% rain");
    assert_eq!(text("de"), "40 % Regen");
  }

  /// Our `gpui_component:` texts hold every built-in component text in English,
  /// unchanged, and in every other language
  #[test]
  fn components_are_complete() {
    let builtin: HashMap<String, String> = gpui_kit::component::_rust_i18n_backend()
      .messages_for_locale("en")
      .unwrap_or_default()
      .into_iter()
      .map(|(key, text)| (format!("gpui_component.{key}"), text.into_owned()))
      .collect();
    assert!(builtin.len() > 70, "{}", builtin.len());
    assert_eq!(messages("en", "gpui_component."), builtin);

    for locale in super::languages() {
      let ours = messages(&locale, "gpui_component.");
      let mut missing: Vec<_> = builtin.keys().filter(|k| !ours.contains_key(*k)).collect();
      missing.sort();
      assert!(missing.is_empty(), "missing in {locale}: {missing:?}");
    }
    let de = messages("de", "gpui_component.");
    assert_eq!(de["gpui_component.Settings.Reset All"], "Alle zurücksetzen");
  }
}
