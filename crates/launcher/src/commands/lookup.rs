//! Translate through DeepL and define words through Wiktionary

use std::{rc::Rc, sync::Arc};

use anyhow::{Context, Result, bail};
use corona_config::{ConfigProvider, launcher::TranslateConfig};
use futures::AsyncReadExt;
use gpui_kit::{
  App, SharedString,
  assets::IconName,
  http_client::{AsyncBody, HttpClient, Method, Request},
};
use rust_i18n::t;
use serde::Deserialize;

use crate::{
  args::{ArgSpec, Args},
  command::{Action, Clip, Command, Icon, Meta, Outcome},
  commands::web::encode,
  registry,
  view::{DetailView, View},
};

/// The key from its file, else from the config
fn deepl_key(config: &TranslateConfig) -> Result<String> {
  if let Some(file) = &config.deepl_key_file {
    let key = std::fs::read_to_string(corona_config::expand_home(file))
      .with_context(|| format!("reading {}", file.display()))?;
    return Ok(key.trim().to_string());
  }
  config
    .deepl_key
    .clone()
    .filter(|k| !k.trim().is_empty())
    .context(t!("app.launcher.lookup.no_key").to_string())
}

async fn body(client: &Arc<dyn HttpClient>, request: Request<AsyncBody>) -> Result<Vec<u8>> {
  let mut response = client.send(request).await?;
  let status = response.status();
  let mut bytes = Vec::new();
  response.body_mut().read_to_end(&mut bytes).await?;
  if !status.is_success() {
    bail!("{status}: {}", String::from_utf8_lossy(&bytes));
  }
  Ok(bytes)
}

#[derive(Deserialize)]
struct Translations {
  translations: Vec<Translation>,
}

#[derive(Deserialize)]
struct Translation {
  detected_source_language: String,
  text: String,
}

async fn translate(
  client: Arc<dyn HttpClient>,
  key: String,
  text: String,
  target: String,
) -> Result<(String, String)> {
  // free keys end in `:fx` and have their own host
  let host = match key.ends_with(":fx") {
    true => "api-free.deepl.com",
    false => "api.deepl.com",
  };
  let form = format!(
    "text={}&target_lang={}",
    encode(&text),
    encode(&target.to_uppercase())
  );
  let request = Request::builder()
    .method(Method::POST)
    .uri(format!("https://{host}/v2/translate"))
    .header("Authorization", format!("DeepL-Auth-Key {key}"))
    .header("Content-Type", "application/x-www-form-urlencoded")
    .body(AsyncBody::from(form))?;
  let parsed: Translations = serde_json::from_slice(&body(&client, request).await?)?;
  let first = parsed
    .translations
    .into_iter()
    .next()
    .context("no translation")?;
  Ok((first.text, first.detected_source_language))
}

/// Wiktionary's definitions: part of speech, then the definitions without markup
#[derive(Deserialize)]
struct Usage {
  #[serde(rename = "partOfSpeech")]
  part_of_speech: String,
  definitions: Vec<Definition>,
}

#[derive(Deserialize)]
struct Definition {
  definition: String,
}

fn strip_tags(html: &str) -> String {
  let mut out = String::new();
  let mut tag = false;
  for c in html.chars() {
    match c {
      '<' => tag = true,
      '>' => tag = false,
      c if !tag => out.push(c),
      _ => {}
    }
  }
  out
    .replace("&nbsp;", " ")
    .replace("&amp;", "&")
    .replace("&quot;", "\"")
    .replace("&#39;", "'")
}

/// The definitions as markdown, English entries first
pub fn definitions_markdown(word: &str, json: &[u8]) -> Result<String> {
  let entries: std::collections::BTreeMap<String, Vec<Usage>> = serde_json::from_slice(json)?;
  let mut md = format!("# {word}\n");
  let mut languages: Vec<(&String, &Vec<Usage>)> = entries.iter().collect();
  languages.sort_by_key(|(lang, _)| *lang != "en");
  for (_, usages) in languages.into_iter().take(2) {
    for usage in usages {
      md.push_str(&format!("\n**{}**\n", usage.part_of_speech));
      for (i, d) in usage
        .definitions
        .iter()
        .filter(|d| !d.definition.trim().is_empty())
        .take(5)
        .enumerate()
      {
        md.push_str(&format!(
          "\n{}. {}",
          i + 1,
          strip_tags(&d.definition).trim()
        ));
      }
      md.push('\n');
    }
  }
  Ok(md)
}

async fn define(client: Arc<dyn HttpClient>, word: String) -> Result<String> {
  let url = format!(
    "https://en.wiktionary.org/api/rest_v1/page/definition/{}",
    encode(&word)
  );
  let request = Request::builder()
    .method(Method::GET)
    .uri(url)
    .header("Accept", "application/json")
    .body(AsyncBody::empty())?;
  definitions_markdown(
    &word,
    &body(&client, request)
      .await
      .context(t!("app.launcher.lookup.not_found").to_string())?,
  )
}

/// The argument, else the selected text, else the clipboard
fn input(args: &Args, cx: &App) -> Option<String> {
  args
    .text(0)
    .map(str::to_string)
    .or_else(|| cx.read_from_primary().and_then(|i| i.text()))
    .or_else(|| cx.read_from_clipboard().and_then(|i| i.text()))
    .filter(|t| !t.trim().is_empty())
}

fn translate_command() -> Rc<dyn Command> {
  crate::simple(
    "translate",
    |_| Meta {
      title: t!("app.launcher.lookup.translate").into(),
      icon: Icon::Tile(IconName::Languages, 230.),
      keywords: vec!["deepl".into(), "language".into()],
      args: vec![
        ArgSpec::text(t!("app.launcher.lookup.text")).optional(),
        ArgSpec::text(t!("app.launcher.lookup.target")).optional(),
      ],
      args_row: true,
      ..Default::default()
    },
    |_| {
      vec![Action::spawn(
        "translate",
        t!("app.launcher.lookup.translate"),
        |args, cx| {
          let config = cx.config().launcher.translate.clone();
          let target = args
            .text(1)
            .map(str::to_string)
            .unwrap_or(config.target.clone());
          let (key, text) = (deepl_key(&config), input(args, cx));
          let client = cx.http_client();
          async move {
            let text = text.context(t!("app.launcher.lookup.nothing").to_string())?;
            let (translated, source) =
              translate(client, key?, text.clone(), target.clone()).await?;
            let copy = translated.clone();
            Ok(Outcome::Push(View::Detail(DetailView {
              title: t!("app.launcher.lookup.translate").into(),
              markdown: format!("{translated}\n\n---\n\n{text}").into(),
              metadata: vec![
                (t!("app.launcher.lookup.from").into(), source.into()),
                (t!("app.launcher.lookup.to").into(), target.into()),
              ],
              actions: vec![Action::sync(
                "copy",
                t!("app.launcher.lookup.copy"),
                move |_, _, _| Ok(Outcome::Copy(Clip::Text(copy.clone()))),
              )],
            })))
          }
        },
      )]
    },
  )
}

fn define_command() -> Rc<dyn Command> {
  crate::simple(
    "define",
    |_| Meta {
      title: t!("app.launcher.lookup.define").into(),
      icon: Icon::Tile(IconName::BookOpen, 30.),
      keywords: vec!["dictionary".into(), "meaning".into(), "wiktionary".into()],
      args: vec![ArgSpec::text(t!("app.launcher.lookup.word")).optional()],
      ..Default::default()
    },
    |_| {
      vec![Action::spawn(
        "define",
        t!("app.launcher.lookup.define"),
        |args, cx| {
          let word = input(args, cx);
          let client = cx.http_client();
          async move {
            let word = word.context(t!("app.launcher.lookup.nothing").to_string())?;
            let word = word
              .split_whitespace()
              .next()
              .unwrap_or_default()
              .to_string();
            let markdown = define(client, word.clone()).await?;
            let url = format!("https://en.wiktionary.org/wiki/{}", encode(&word));
            Ok(Outcome::Push(View::Detail(DetailView {
              title: SharedString::from(word),
              markdown: markdown.into(),
              metadata: Vec::new(),
              actions: vec![Action::sync(
                "open",
                t!("app.launcher.web.open"),
                move |_, _, cx| {
                  cx.open_url(&url);
                  Ok(Outcome::close())
                },
              )],
            })))
          }
        },
      )]
    },
  )
}

pub fn init(cx: &mut App) {
  registry::register_all([translate_command(), define_command()], cx);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn wiktionary() {
    let json = br#"{"en":[{"partOfSpeech":"Noun","definitions":[{"definition":"A <a href=\"x\">small</a> cat."},{"definition":""}]}],
      "de":[{"partOfSpeech":"Substantiv","definitions":[{"definition":"Katze"}]}]}"#;
    let md = definitions_markdown("cat", json).unwrap();
    assert!(
      md.starts_with("# cat\n\n**Noun**\n\n1. A small cat."),
      "{md}"
    );
    assert!(md.contains("Katze"));
  }

  #[test]
  fn keys() {
    let mut config = TranslateConfig::default();
    assert!(deepl_key(&config).is_err());
    config.deepl_key = Some("abc:fx".into());
    assert_eq!(deepl_key(&config).unwrap(), "abc:fx");
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("key");
    std::fs::write(&file, "fromfile\n").unwrap();
    config.deepl_key_file = Some(file);
    assert_eq!(deepl_key(&config).unwrap(), "fromfile");
  }
}
