//! Emoji grid with recent ones and a remembered skin tone, and Unicode symbol search

use std::{rc::Rc, sync::OnceLock};

use emojis::{Emoji, SkinTone};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::{
  command::{Action, Clip, Command, Icon, Meta, Outcome},
  kit::{Choices, ListSource, opener},
  matching::Query,
  registry, state,
  view::ListView,
};

const RECENT: usize = 16;
const SYMBOLS: usize = 200;

#[derive(Default, Serialize, Deserialize)]
struct Remembered {
  recent: Vec<String>,
  /// Index into [`TONES`]
  tone: usize,
}

const TONES: [SkinTone; 6] = [
  SkinTone::Default,
  SkinTone::Light,
  SkinTone::MediumLight,
  SkinTone::Medium,
  SkinTone::MediumDark,
  SkinTone::Dark,
];

fn remembered() -> Remembered {
  state::load("emoji")
}

fn toned(emoji: &'static Emoji, tone: usize) -> &'static Emoji {
  emoji
    .with_skin_tone(TONES[tone.min(TONES.len() - 1)])
    .unwrap_or(emoji)
}

fn used(text: &str) {
  let mut r = remembered();
  r.recent.retain(|e| e != text);
  r.recent.insert(0, text.to_string());
  r.recent.truncate(RECENT);
  let _ = state::save("emoji", &r);
}

fn emoji_row(emoji: &'static Emoji) -> Rc<dyn Command> {
  let text = emoji.as_str().to_string();
  simple_glyph(
    format!("emoji:{}", emoji.as_str()),
    text,
    emoji.name().to_string(),
    emoji.shortcodes().map(|s| s.to_string()).collect(),
  )
}

/// A character to paste: Enter pastes, Ctrl+Enter copies
fn simple_glyph(id: String, text: String, name: String, keywords: Vec<String>) -> Rc<dyn Command> {
  let shown = text.clone();
  crate::simple(
    id,
    move |_| Meta {
      title: name.clone().into(),
      icon: Icon::Glyph(shown.clone().into()),
      keywords: keywords.iter().cloned().map(Into::into).collect(),
      ..Default::default()
    },
    move |_| {
      let (paste, copy) = (text.clone(), text.clone());
      vec![
        Action::sync("paste", t!("app.launcher.emoji.paste"), move |_, _, _| {
          used(&paste);
          Ok(Outcome::Paste(Clip::Text(paste.clone())))
        }),
        Action::sync("copy", t!("app.launcher.emoji.copy"), move |_, _, _| {
          used(&copy);
          Ok(Outcome::Copy(Clip::Text(copy.clone())))
        })
        .shortcut("ctrl-enter"),
        Action::sync("tone", t!("app.launcher.emoji.skin_tone"), |_, _, _| {
          Ok(Outcome::Push(tones()))
        })
        .icon(IconName::Hand),
      ]
    },
  )
}

fn tones() -> crate::View {
  let names = [
    t!("app.launcher.emoji.tone_default"),
    t!("app.launcher.emoji.tone_light"),
    t!("app.launcher.emoji.tone_medium_light"),
    t!("app.launcher.emoji.tone_medium"),
    t!("app.launcher.emoji.tone_medium_dark"),
    t!("app.launcher.emoji.tone_dark"),
  ];
  let hand = emojis::get("👋").expect("waving hand is an emoji");
  Choices {
    id: "emoji:tone",
    items: Rc::new(move |_| {
      names
        .iter()
        .enumerate()
        .map(|(i, n)| {
          (
            i.to_string().into(),
            format!("{} {n}", toned(hand, i).as_str()).into(),
          )
        })
        .collect()
    }),
    current: Rc::new(|_| Some(remembered().tone.to_string().into())),
    apply: Rc::new(|tone, _| {
      let mut r = remembered();
      r.tone = tone.parse().unwrap_or(0);
      state::save("emoji", &r)
    }),
    preview: None,
    icon: Icon::None,
  }
  .view(
    t!("app.launcher.emoji.skin_tone"),
    t!("app.launcher.emoji.search"),
  )
}

/// Emoji whose name or shortcodes have every word of `query`, recent ones first when empty
fn emojis_for(query: &Query) -> Vec<Rc<dyn Command>> {
  let r = remembered();
  if query.is_empty() {
    let recent = r
      .recent
      .iter()
      .filter_map(|e| emojis::get(e))
      .map(|e| emoji_row(e));
    let rest = emojis::iter()
      .filter(|e| !r.recent.iter().any(|x| x == e.as_str()))
      .map(|e| emoji_row(toned(e, r.tone)));
    return recent.chain(rest).collect();
  }
  let words: Vec<&str> = query.text.split_whitespace().collect();
  emojis::iter()
    .filter(|e| {
      let name = e.name().to_lowercase();
      words.iter().all(|w| {
        name.split_whitespace().any(|n| n.starts_with(w)) || e.shortcodes().any(|s| s.contains(w))
      })
    })
    .map(|e| emoji_row(toned(e, r.tone)))
    .collect()
}

/// Every named character outside the large ideograph and private blocks
fn symbol_names() -> &'static Vec<(char, String)> {
  static NAMES: OnceLock<Vec<(char, String)>> = OnceLock::new();
  NAMES.get_or_init(|| {
    let ranges = [
      0x00A0..0x0250u32,
      0x0370..0x0400,
      0x2000..0x2C00,
      0x2E00..0x2E80,
      0x3000..0x3040,
      0xFE30..0xFE50,
      0xFF00..0xFFF0,
      0x1D400..0x1D800,
      0x1F000..0x1F100,
    ];
    ranges
      .into_iter()
      .flatten()
      .filter_map(|c| {
        let c = char::from_u32(c)?;
        Some((c, unicode_names2::name(c)?.to_string().to_lowercase()))
      })
      .collect()
  })
}

fn symbols_for(query: &Query) -> Vec<Rc<dyn Command>> {
  if query.is_empty() {
    return Vec::new();
  }
  let words: Vec<&str> = query.text.split_whitespace().collect();
  symbol_names()
    .iter()
    .filter(|(_, name)| {
      words
        .iter()
        .all(|w| name.split_whitespace().any(|n| n.starts_with(w)))
    })
    .take(SYMBOLS)
    .map(|(c, name)| {
      simple_glyph(
        format!("symbol:{}", *c as u32),
        c.to_string(),
        format!("{name} (U+{:04X})", *c as u32),
        Vec::new(),
      )
    })
    .collect()
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      opener(
        "emoji",
        t!("app.launcher.emoji.title"),
        Icon::Glyph("😀".into()),
        &["emoji", "smiley", "emoticon"],
        |_| {
          ListView::new(
            t!("app.launcher.emoji.title"),
            t!("app.launcher.emoji.search"),
            Rc::new(ListSource::new("emoji", |q, _| emojis_for(q)).filters()),
          )
          .grid(8, 50., false)
          .into_view()
        },
      ),
      opener(
        "symbols",
        t!("app.launcher.emoji.symbols"),
        Icon::Glyph("→".into()),
        &["unicode", "character", "arrow", "degree"],
        |_| {
          ListView::new(
            t!("app.launcher.emoji.symbols"),
            t!("app.launcher.emoji.search_symbols"),
            Rc::new(ListSource::new("symbols", |q, _| symbols_for(q)).filters()),
          )
          .empty(t!("app.launcher.emoji.type_name"))
          .into_view()
        },
      ),
    ],
    cx,
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_symbols_by_name() {
    let names = symbol_names();
    let find = |words: &[&str]| {
      names
        .iter()
        .find(|(_, n)| {
          words
            .iter()
            .all(|w| n.split_whitespace().any(|x| x.starts_with(w)))
        })
        .map(|(c, _)| *c)
    };
    assert_eq!(find(&["degree", "sign"]), Some('°'));
    assert_eq!(find(&["rightwards", "arrow"]), Some('→'));
  }

  #[test]
  fn skin_tones() {
    let hand = emojis::get("👋").unwrap();
    assert_eq!(toned(hand, 0).as_str(), "👋");
    assert_ne!(toned(hand, 3).as_str(), "👋");
    // emoji without tones stay as they are
    let cat = emojis::get("🐈").unwrap();
    assert_eq!(toned(cat, 3).as_str(), "🐈");
  }
}
