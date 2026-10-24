use std::time::Duration;

use anyhow::{Context, Result, bail};
use corona_utils::parse_level;
use gpui_kit::SharedString;

#[derive(Clone, Debug, PartialEq)]
pub enum ArgKind {
  /// Free text, the last text argument takes the rest of the input
  Text,
  Number,
  /// `65`, `65%`, `0.65`, or a step `+5` / `-5`
  Level,
  /// `5m`, `1h30m`, `90s`; a bare number is minutes
  Duration,
  /// One of fixed values
  Choice(Vec<SharedString>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArgSpec {
  pub name: SharedString,
  pub kind: ArgKind,
  pub optional: bool,
  /// Literal words accepted next to the kind, like `mute` for a level
  pub words: Vec<SharedString>,
}

impl ArgSpec {
  fn new(name: impl Into<SharedString>, kind: ArgKind) -> Self {
    Self {
      name: name.into(),
      kind,
      optional: false,
      words: Vec::new(),
    }
  }

  pub fn text(name: impl Into<SharedString>) -> Self {
    Self::new(name, ArgKind::Text)
  }

  pub fn number(name: impl Into<SharedString>) -> Self {
    Self::new(name, ArgKind::Number)
  }

  pub fn level(name: impl Into<SharedString>) -> Self {
    Self::new(name, ArgKind::Level)
  }

  pub fn duration(name: impl Into<SharedString>) -> Self {
    Self::new(name, ArgKind::Duration)
  }

  pub fn choice<S: Into<SharedString>>(
    name: impl Into<SharedString>,
    values: impl IntoIterator<Item = S>,
  ) -> Self {
    Self::new(
      name,
      ArgKind::Choice(values.into_iter().map(Into::into).collect()),
    )
  }

  pub fn optional(mut self) -> Self {
    self.optional = true;
    self
  }

  pub fn words<S: Into<SharedString>>(mut self, words: impl IntoIterator<Item = S>) -> Self {
    self.words = words.into_iter().map(Into::into).collect();
    self
  }

  /// Values Tab cycles through for a partial `input`
  pub fn completions(&self, input: &str) -> Vec<SharedString> {
    let input = input.to_lowercase();
    let choices = match &self.kind {
      ArgKind::Choice(values) => values.as_slice(),
      _ => &[],
    };
    choices
      .iter()
      .chain(&self.words)
      .filter(|v| v.to_lowercase().starts_with(&input))
      .cloned()
      .collect()
  }

  pub fn parse(&self, input: &str) -> Result<ArgValue> {
    let input = input.trim();
    if let Some(word) = self.words.iter().find(|w| w.eq_ignore_ascii_case(input)) {
      return Ok(ArgValue::Word(word.clone()));
    }
    match &self.kind {
      ArgKind::Text => Ok(ArgValue::Text(input.to_string())),
      ArgKind::Number => input
        .parse()
        .map(ArgValue::Number)
        .context("expected a number"),
      ArgKind::Level => parse_step(input),
      ArgKind::Duration => parse_duration(input).map(ArgValue::Duration),
      ArgKind::Choice(values) => values
        .iter()
        .find(|v| v.eq_ignore_ascii_case(input))
        .map(|v| ArgValue::Choice(v.clone()))
        .with_context(|| format!("expected one of {}", join(values))),
    }
  }
}

fn join(values: &[SharedString]) -> String {
  values
    .iter()
    .map(|v| v.as_ref())
    .collect::<Vec<_>>()
    .join(", ")
}

fn parse_step(input: &str) -> Result<ArgValue> {
  if let Some(rest) = input.strip_prefix('+') {
    return Ok(ArgValue::Step(parse_level(rest)?));
  }
  if let Some(rest) = input.strip_prefix('-') {
    return Ok(ArgValue::Step(-parse_level(rest)?));
  }
  Ok(ArgValue::Level(parse_level(input)?))
}

/// `5m`, `1h30m`, `90s`, `1h 5m`; a bare number is minutes
pub fn parse_duration(input: &str) -> Result<Duration> {
  let input: String = input.chars().filter(|c| !c.is_whitespace()).collect();
  if input.is_empty() {
    bail!("expected a duration like 5m or 1h30m");
  }
  if let Ok(minutes) = input.parse::<u64>() {
    return Ok(Duration::from_secs(minutes * 60));
  }
  let mut total = 0u64;
  let mut number = String::new();
  for c in input.chars() {
    if c.is_ascii_digit() {
      number.push(c);
      continue;
    }
    let unit = match c {
      'h' => 3600,
      'm' => 60,
      's' => 1,
      _ => bail!("expected a duration like 5m or 1h30m"),
    };
    let n: u64 = number
      .parse()
      .context("expected a duration like 5m or 1h30m")?;
    total += n * unit;
    number.clear();
  }
  if !number.is_empty() || total == 0 {
    bail!("expected a duration like 5m or 1h30m");
  }
  Ok(Duration::from_secs(total))
}

#[derive(Clone, Debug, PartialEq)]
pub enum ArgValue {
  Text(String),
  Number(f64),
  Level(f32),
  /// Signed change like `+5` as 0.05
  Step(f32),
  Duration(Duration),
  Choice(SharedString),
  Word(SharedString),
}

impl ArgValue {
  pub fn as_str(&self) -> Option<&str> {
    match self {
      ArgValue::Text(s) => Some(s),
      ArgValue::Choice(s) | ArgValue::Word(s) => Some(s),
      _ => None,
    }
  }
}

/// Parsed arguments in spec order, `None` for an omitted optional one
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args(pub Vec<Option<ArgValue>>);

impl Args {
  pub fn get(&self, i: usize) -> Option<&ArgValue> {
    self.0.get(i).and_then(Option::as_ref)
  }

  pub fn text(&self, i: usize) -> Option<&str> {
    self.get(i).and_then(ArgValue::as_str)
  }

  /// Splits `input` over `specs`: whitespace separated, the last spec takes the rest
  pub fn parse(specs: &[ArgSpec], input: &str) -> Result<Self> {
    let mut rest = input.trim();
    let mut values = Vec::with_capacity(specs.len());
    for (i, spec) in specs.iter().enumerate() {
      let last = i + 1 == specs.len();
      let part = match last {
        true => std::mem::take(&mut rest),
        false => match rest.split_once(char::is_whitespace) {
          Some((part, tail)) => {
            rest = tail.trim_start();
            part
          }
          None => std::mem::take(&mut rest),
        },
      };
      if part.is_empty() {
        if !spec.optional {
          bail!("{} is required", spec.name);
        }
        values.push(None);
        continue;
      }
      values.push(Some(spec.parse(part)?));
    }
    Ok(Self(values))
  }

  /// Fields filled from the arguments row, one input per spec
  pub fn from_fields(specs: &[ArgSpec], fields: &[String]) -> Result<Self> {
    let mut values = Vec::with_capacity(specs.len());
    for (spec, field) in specs.iter().zip(
      fields
        .iter()
        .map(String::as_str)
        .chain(std::iter::repeat("")),
    ) {
      if field.trim().is_empty() {
        if !spec.optional {
          bail!("{} is required", spec.name);
        }
        values.push(None);
      } else {
        values.push(Some(spec.parse(field)?));
      }
    }
    Ok(Self(values))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn durations() {
    let d = |s| parse_duration(s).unwrap().as_secs();
    assert_eq!(d("30"), 1800);
    assert_eq!(d("5m"), 300);
    assert_eq!(d("1h30m"), 5400);
    assert_eq!(d("1h 5m"), 3900);
    assert_eq!(d("90s"), 90);
    for bad in ["", "5x", "m", "1h30", "0s"] {
      assert!(parse_duration(bad).is_err(), "{bad:?}");
    }
  }

  #[test]
  fn levels_and_steps() {
    let spec = ArgSpec::level("level").words(["mute"]);
    assert_eq!(spec.parse("40").unwrap(), ArgValue::Level(0.4));
    assert_eq!(spec.parse("+5").unwrap(), ArgValue::Step(0.05));
    assert_eq!(spec.parse("-5%").unwrap(), ArgValue::Step(-0.05));
    assert_eq!(spec.parse("MUTE").unwrap(), ArgValue::Word("mute".into()));
    assert!(spec.parse("loud").is_err());
  }

  #[test]
  fn choices_and_completion() {
    let spec = ArgSpec::choice("state", ["on", "off", "toggle"]);
    assert_eq!(spec.parse("Off").unwrap(), ArgValue::Choice("off".into()));
    assert!(spec.parse("maybe").is_err());
    assert_eq!(spec.completions("o"), vec!["on", "off"]);
    assert_eq!(spec.completions("t"), vec!["toggle"]);
  }

  #[test]
  fn split_args() {
    let specs = [
      ArgSpec::duration("duration"),
      ArgSpec::text("label").optional(),
    ];
    let args = Args::parse(&specs, "5m tea is ready").unwrap();
    assert_eq!(
      args.get(0),
      Some(&ArgValue::Duration(Duration::from_secs(300)))
    );
    assert_eq!(args.text(1), Some("tea is ready"));
    let args = Args::parse(&specs, "5m").unwrap();
    assert_eq!(args.get(1), None);
    assert!(Args::parse(&specs, "").is_err());
  }

  #[test]
  fn fields() {
    let specs = [ArgSpec::text("name"), ArgSpec::number("n").optional()];
    let args = Args::from_fields(&specs, &["a b".into()]).unwrap();
    assert_eq!(args.text(0), Some("a b"));
    assert_eq!(args.get(1), None);
    assert!(Args::from_fields(&specs, &[]).is_err());
  }
}
