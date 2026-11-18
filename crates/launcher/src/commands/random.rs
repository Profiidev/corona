//! Random data: UUIDs, passwords, passphrases, numbers, dice, a coin, lorem ipsum

use std::{rc::Rc, sync::LazyLock};

use anyhow::{Context, Result, bail};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use super::lookup::escape_markdown;
use crate::{
  args::{ArgSpec, ArgValue, Args},
  command::{Action, Clip, Command, Icon, Meta, Outcome},
  registry,
  view::{DetailView, View, detail_row},
};

/// Uniform in `0..n` from the OS generator, rejection sampled so there is no modulo bias
fn rand_below(n: u64) -> Result<u64> {
  assert!(n > 0, "empty range");
  let limit = u64::MAX - u64::MAX % n;
  loop {
    let x = getrandom::u64()?;
    if x < limit {
      return Ok(x % n);
    }
  }
}

/// Uniform in `min..=max`, either order
fn rand_between(a: i64, b: i64) -> Result<i64> {
  let (min, max) = (a.min(b), a.max(b));
  let span = max.abs_diff(min);
  let offset = match span.checked_add(1) {
    Some(n) => rand_below(n)?,
    None => getrandom::u64()?,
  };
  Ok(min.wrapping_add(offset as i64))
}

fn pick<T: Copy>(items: &[T]) -> Result<T> {
  Ok(items[rand_below(items.len() as u64)? as usize])
}

fn shuffle<T>(items: &mut [T]) -> Result<()> {
  for i in (1..items.len()).rev() {
    items.swap(i, rand_below(i as u64 + 1)? as usize);
  }
  Ok(())
}

const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!@#$%^&*()-_=+[]{};:,.<>?/~";

/// `len` characters with at least one of each class
fn password(len: usize) -> Result<String> {
  let classes = [LOWER, UPPER, DIGITS, SYMBOLS];
  let all: Vec<u8> = classes.concat();
  let mut out = classes
    .iter()
    .map(|c| pick(c))
    .collect::<Result<Vec<_>>>()?;
  while out.len() < len {
    out.push(pick(&all)?);
  }
  shuffle(&mut out)?;
  Ok(String::from_utf8(out).expect("ascii"))
}

/// The EFF large wordlist, 7776 words, about 12.9 bits each
static EFF_WORDS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
  include_str!("../../../../assets/wordlists/eff_large_wordlist.txt")
    .lines()
    .collect()
});

fn passphrase(words: usize) -> Result<String> {
  let words = (0..words)
    .map(|_| pick(&EFF_WORDS))
    .collect::<Result<Vec<_>>>()?;
  Ok(words.join("-"))
}

/// `NdM` like `2d6`, a bare `d20` is one die
fn parse_dice(input: &str) -> Result<(u64, u64)> {
  let (n, m) = input
    .trim()
    .to_ascii_lowercase()
    .split_once('d')
    .map(|(n, m)| (n.to_string(), m.to_string()))
    .context(t!("app.launcher.random.bad_dice").to_string())?;
  let n = if n.is_empty() { Ok(1) } else { n.parse() };
  match (n, m.parse()) {
    (Ok(n @ 1..=100), Ok(m @ 2..=1000)) => Ok((n, m)),
    _ => bail!("{}", t!("app.launcher.random.bad_dice")),
  }
}

fn roll(n: u64, m: u64) -> Result<Vec<u64>> {
  (0..n).map(|_| Ok(rand_below(m)? + 1)).collect()
}

const LATIN: &[&str] = &[
  "lorem",
  "ipsum",
  "dolor",
  "sit",
  "amet",
  "consectetur",
  "adipiscing",
  "elit",
  "sed",
  "do",
  "eiusmod",
  "tempor",
  "incididunt",
  "ut",
  "labore",
  "et",
  "dolore",
  "magna",
  "aliqua",
  "enim",
  "ad",
  "minim",
  "veniam",
  "quis",
  "nostrud",
  "exercitation",
  "ullamco",
  "laboris",
  "nisi",
  "aliquip",
  "ex",
  "ea",
  "commodo",
  "consequat",
  "duis",
  "aute",
  "irure",
  "in",
  "reprehenderit",
  "voluptate",
  "velit",
  "esse",
  "cillum",
  "eu",
  "fugiat",
  "nulla",
  "pariatur",
  "excepteur",
  "sint",
  "occaecat",
  "cupidatat",
  "non",
  "proident",
  "sunt",
  "culpa",
  "qui",
  "officia",
  "deserunt",
  "mollit",
  "anim",
  "id",
  "est",
  "laborum",
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Unit {
  Words,
  Sentences,
  Paragraphs,
}

/// Starts with the classic "lorem ipsum dolor sit amet", then random words
struct Latin(usize);

impl Latin {
  fn word(&mut self) -> Result<&'static str> {
    self.0 += 1;
    match LATIN.get(self.0 - 1).filter(|_| self.0 <= 5) {
      Some(w) => Ok(w),
      None => pick(LATIN),
    }
  }

  fn words(&mut self, n: usize) -> Result<Vec<&'static str>> {
    (0..n).map(|_| self.word()).collect()
  }

  fn sentence(&mut self) -> Result<String> {
    let n = rand_between(4, 12)? as usize;
    let text = self.words(n)?.join(" ");
    let mut chars = text.chars();
    let first = chars.next().expect("at least four words");
    Ok(format!("{}{}.", first.to_uppercase(), chars.as_str()))
  }

  fn sentences(&mut self, n: usize) -> Result<String> {
    Ok(
      (0..n)
        .map(|_| self.sentence())
        .collect::<Result<Vec<_>>>()?
        .join(" "),
    )
  }
}

fn lorem(count: usize, unit: Unit) -> Result<String> {
  let mut latin = Latin(0);
  match unit {
    Unit::Words => Ok(latin.words(count)?.join(" ")),
    Unit::Sentences => latin.sentences(count),
    Unit::Paragraphs => {
      let paragraphs = (0..count)
        .map(|_| {
          let n = rand_between(3, 6)? as usize;
          latin.sentences(n)
        })
        .collect::<Result<Vec<_>>>()?;
      Ok(paragraphs.join("\n\n"))
    }
  }
}

/// One generated value: what copy and paste use, how the detail view shows it
struct Generated {
  value: String,
  markdown: String,
  metadata: Vec<(SharedString, SharedString)>,
}

impl Generated {
  /// Monospace, for tokens like a password
  fn code(value: String) -> Self {
    Self {
      markdown: format!("```\n{value}\n```"),
      value,
      metadata: Vec::new(),
    }
  }

  /// A big heading, for a number or a coin side
  fn big(value: String) -> Self {
    Self {
      markdown: format!("# {}", escape_markdown(&value)),
      value,
      metadata: Vec::new(),
    }
  }

  fn text(value: String) -> Self {
    Self {
      markdown: escape_markdown(&value),
      value,
      metadata: Vec::new(),
    }
  }
}

type Generate = Rc<dyn Fn() -> Result<Generated>>;

fn detail(title: SharedString, generate: Generate) -> Result<View> {
  let Generated {
    value,
    markdown,
    metadata,
  } = generate()?;
  let copy = value.clone();
  let paste = value;
  Ok(View::Detail(DetailView {
    title: title.clone(),
    image: None,
    markdown: markdown.into(),
    metadata,
    actions: vec![
      Action::sync("copy", t!("app.launcher.random.copy"), move |_, _, _| {
        Ok(Outcome::Copy(Clip::Text(copy.clone())))
      })
      .icon(IconName::Copy),
      Action::sync("paste", t!("app.launcher.random.paste"), move |_, _, _| {
        Ok(Outcome::Paste(Clip::Text(paste.clone())))
      })
      .icon(IconName::ClipboardPaste),
      Action::sync(
        "regenerate",
        t!("app.launcher.random.regenerate"),
        move |_, _, _| Ok(Outcome::Replace(detail(title.clone(), generate.clone())?)),
      )
      .icon(IconName::RefreshCw)
      .shortcut("ctrl-r"),
    ],
  }))
}

/// A whole number argument, rounded
fn int(args: &Args, i: usize) -> Option<i64> {
  match args.get(i) {
    Some(ArgValue::Number(n)) => Some(n.round() as i64),
    _ => None,
  }
}

fn count(args: &Args, i: usize, default: usize, max: usize) -> usize {
  int(args, i).map_or(default, |n| n.clamp(1, max as i64) as usize)
}

struct Spec {
  id: &'static str,
  title: SharedString,
  icon: IconName,
  hue: f32,
  keywords: &'static [&'static str],
  args: Vec<ArgSpec>,
  /// Reads the arguments once, the result generates every value
  make: fn(&Args, &App) -> Result<Generate>,
}

fn command(spec: Spec) -> Rc<dyn Command> {
  let Spec {
    id,
    title,
    icon,
    hue,
    keywords,
    args,
    make,
  } = spec;
  let action_title = title.clone();
  crate::simple(
    id,
    move |_| Meta {
      title: title.clone(),
      icon: Icon::Tile(icon, hue),
      keywords: keywords.iter().map(|&k| k.into()).collect(),
      args: args.clone(),
      ..Default::default()
    },
    move |_| {
      let title = action_title.clone();
      vec![Action::sync(
        id,
        t!("app.launcher.random.generate"),
        move |args, _, cx| Ok(Outcome::Push(detail(title.clone(), make(args, cx)?)?)),
      )]
    },
  )
}

fn units() -> [SharedString; 3] {
  [
    t!("app.launcher.random.words").into(),
    t!("app.launcher.random.sentences").into(),
    t!("app.launcher.random.paragraphs").into(),
  ]
}

fn commands() -> Vec<Rc<dyn Command>> {
  let specs = [
    Spec {
      id: "uuid",
      title: t!("app.launcher.random.uuid").into(),
      icon: IconName::FingerprintPattern,
      hue: 260.,
      keywords: &["guid", "id", "random"],
      args: vec![ArgSpec::choice(t!("app.launcher.random.version"), ["v4", "v7"]).optional()],
      make: |args, _| {
        let v7 = args.text(0) == Some("v7");
        Ok(Rc::new(move || {
          let id = if v7 {
            uuid::Uuid::now_v7()
          } else {
            uuid::Uuid::new_v4()
          };
          Ok(Generated::code(id.to_string()))
        }))
      },
    },
    Spec {
      id: "password",
      title: t!("app.launcher.random.password").into(),
      icon: IconName::KeyRound,
      hue: 30.,
      keywords: &["generate", "secret", "random"],
      args: vec![ArgSpec::number(t!("app.launcher.random.length")).optional()],
      make: |args, _| {
        let len = int(args, 0).map_or(20, |n| n.clamp(4, 256) as usize);
        Ok(Rc::new(move || Ok(Generated::code(password(len)?))))
      },
    },
    Spec {
      id: "passphrase",
      title: t!("app.launcher.random.passphrase").into(),
      icon: IconName::RectangleEllipsis,
      hue: 50.,
      keywords: &["diceware", "password", "words", "random"],
      args: vec![ArgSpec::number(t!("app.launcher.random.word_count")).optional()],
      make: |args, _| {
        let words = count(args, 0, 6, 64);
        Ok(Rc::new(move || Ok(Generated::code(passphrase(words)?))))
      },
    },
    Spec {
      id: "random",
      title: t!("app.launcher.random.number").into(),
      icon: IconName::Hash,
      hue: 190.,
      keywords: &["number", "integer", "range", "rng"],
      args: vec![
        ArgSpec::number(t!("app.launcher.random.min")).optional(),
        ArgSpec::number(t!("app.launcher.random.max")).optional(),
      ],
      make: |args, _| {
        let (a, b) = (int(args, 0).unwrap_or(1), int(args, 1).unwrap_or(100));
        let range = format!("{}–{}", a.min(b), a.max(b));
        Ok(Rc::new(move || {
          let mut out = Generated::big(rand_between(a, b)?.to_string());
          out.metadata = vec![detail_row(t!("app.launcher.random.range"), range.clone())];
          Ok(out)
        }))
      },
    },
    Spec {
      id: "roll",
      title: t!("app.launcher.random.roll").into(),
      icon: IconName::Dices,
      hue: 340.,
      keywords: &["dice", "die", "d6", "d20", "random"],
      args: vec![ArgSpec::text(t!("app.launcher.random.dice")).optional()],
      make: |args, _| {
        let (n, m) = parse_dice(args.text(0).unwrap_or("1d6"))?;
        Ok(Rc::new(move || {
          let dice = roll(n, m)?;
          let sum: u64 = dice.iter().sum();
          let list = (dice.iter().map(u64::to_string))
            .collect::<Vec<_>>()
            .join(", ");
          let mut out = Generated::big(sum.to_string());
          out.metadata = vec![
            detail_row(t!("app.launcher.random.dice"), format!("{n}d{m}")),
            detail_row(t!("app.launcher.random.rolls"), list),
          ];
          Ok(out)
        }))
      },
    },
    Spec {
      id: "coin",
      title: t!("app.launcher.random.coin").into(),
      icon: IconName::Coins,
      hue: 45.,
      keywords: &["flip", "toss", "heads", "tails", "random"],
      args: Vec::new(),
      make: |_, _| {
        Ok(Rc::new(|| {
          let side = if rand_below(2)? == 0 {
            t!("app.launcher.random.heads")
          } else {
            t!("app.launcher.random.tails")
          };
          Ok(Generated::big(side.to_string()))
        }))
      },
    },
    Spec {
      id: "lorem",
      title: t!("app.launcher.random.lorem").into(),
      icon: IconName::Pilcrow,
      hue: 120.,
      keywords: &["ipsum", "placeholder", "dummy", "text"],
      args: vec![
        ArgSpec::number(t!("app.launcher.random.count")).optional(),
        ArgSpec::choice(t!("app.launcher.random.unit"), units()).optional(),
      ],
      make: |args, _| {
        let unit = match args
          .text(1)
          .and_then(|u| units().iter().position(|v| v == u))
        {
          Some(0) => Unit::Words,
          Some(1) => Unit::Sentences,
          _ => Unit::Paragraphs,
        };
        let n = count(args, 0, 1, 100);
        Ok(Rc::new(move || Ok(Generated::text(lorem(n, unit)?))))
      },
    },
  ];
  specs.into_iter().map(command).collect()
}

pub fn init(cx: &mut App) {
  registry::register_all(commands(), cx);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn rand_below_stays_in_range() {
    for n in [1, 2, 3, 7, 1000] {
      for _ in 0..200 {
        assert!(rand_below(n).unwrap() < n);
      }
    }
  }

  #[test]
  fn rand_between_is_inclusive_either_order() {
    for _ in 0..200 {
      assert!((3..=5).contains(&rand_between(5, 3).unwrap()));
    }
    assert_eq!(rand_between(7, 7).unwrap(), 7);
    rand_between(i64::MIN, i64::MAX).unwrap();
  }

  #[test]
  fn password_has_every_class() {
    for _ in 0..100 {
      let p = password(4).unwrap();
      assert_eq!(p.len(), 4);
      for class in [LOWER, UPPER, DIGITS, SYMBOLS] {
        assert!(p.bytes().any(|b| class.contains(&b)), "{p}");
      }
    }
    assert_eq!(password(32).unwrap().len(), 32);
  }

  #[test]
  fn passphrase_words_come_from_the_list() {
    assert_eq!(EFF_WORDS.len(), 7776);
    let p = passphrase(6).unwrap();
    let words: Vec<_> = p.split('-').collect();
    assert_eq!(words.len(), 6);
    assert!(words.iter().all(|w| EFF_WORDS.contains(w)));
  }

  #[test]
  fn dice_parse() {
    assert_eq!(parse_dice("2d6").unwrap(), (2, 6));
    assert_eq!(parse_dice("D20").unwrap(), (1, 20));
    for bad in ["0d6", "d", "2d1", "6", "xdy", "101d6"] {
      assert!(parse_dice(bad).is_err(), "{bad}");
    }
    let dice = roll(3, 6).unwrap();
    assert_eq!(dice.len(), 3);
    assert!(dice.iter().all(|d| (1..=6).contains(d)));
  }

  #[test]
  fn lorem_counts() {
    let words = lorem(8, Unit::Words).unwrap();
    assert!(words.starts_with("lorem ipsum dolor sit amet "));
    assert_eq!(words.split(' ').count(), 8);
    let sentences = lorem(3, Unit::Sentences).unwrap();
    assert!(sentences.starts_with("Lorem ipsum"));
    assert_eq!(sentences.matches('.').count(), 3);
    assert_eq!(lorem(2, Unit::Paragraphs).unwrap().split("\n\n").count(), 2);
  }
}
