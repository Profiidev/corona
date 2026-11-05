use std::{
  cell::RefCell,
  collections::BTreeMap,
  path::{Path, PathBuf},
};

use corona_js::{Cx, Data, Json, ModuleBuilder, Opt, host_module, track};
use gpui_kit::Entity;
use rust_i18n_support::try_load_locales;
use serde_json::{Map, Value};
use specta::specta;

use crate::module::settings::Settings;

/// The fallback when neither the language nor its prefix has a translation
const FALLBACK: &str = "en";

/// A plugin's translations: its `locales/*.yml`, in the format of the shell's
/// own (`_version: 2`, or one file per language)
struct Translations {
  language: String,
  /// The most specific language first, each as `key` -> text
  texts: Vec<BTreeMap<String, String>>,
}

impl Translations {
  fn load(dir: &Path, language: String) -> Self {
    // a link must not lead out of the plugin, `t` would read any file
    let root = dir.parent().and_then(|dir| dir.canonicalize().ok());
    let outside = |path: &str| {
      let file = Path::new(path).canonicalize().ok();
      !file
        .zip(root.as_ref())
        .is_some_and(|(file, root)| file.starts_with(root))
    };
    let mut all = dir
      .to_str()
      .map(|dir| try_load_locales(dir, outside, false))
      .transpose()
      .map_err(|e| tracing::warn!("locales in {}: {e}", dir.display()))
      .ok()
      .flatten()
      .unwrap_or_default();

    let prefix = language.split(['-', '_']).next().unwrap_or_default();
    let mut names = vec![language.as_str(), prefix, FALLBACK];
    names.dedup();
    let texts = names
      .into_iter()
      .filter_map(|name| all.remove(name))
      .collect();
    Self { language, texts }
  }

  fn get(&self, key: &str) -> Option<&str> {
    self
      .texts
      .iter()
      .find_map(|texts| texts.get(key))
      .map(String::as_str)
  }
}

/// `%{name}` in `text` as `vars.name`, in one pass: a value is not searched
/// for `%{other}` again
fn substitute(text: &str, vars: &Map<String, Value>) -> String {
  let (mut out, mut rest) = (String::new(), text);
  while let Some(start) = rest.find("%{") {
    out.push_str(&rest[..start]);
    rest = &rest[start..];
    let var = rest[2..]
      .find('}')
      .and_then(|end| Some((vars.get(&rest[2..end + 2])?, end + 3)));
    let len = match var {
      Some((Value::String(s), len)) => {
        out.push_str(s);
        len
      }
      Some((other, len)) => {
        out.push_str(&other.to_string());
        len
      }
      None => {
        out.push_str("%{");
        2
      }
    };
    rest = &rest[len..];
  }
  out + rest
}

/// A runtime's translations, read once per language: edits show after a reload
struct I18n {
  dir: PathBuf,
  cache: RefCell<Option<Translations>>,
  /// Follows the shell's language
  settings: Entity<Settings>,
}

impl I18n {
  /// The shell's language; a view that reads it renders again as it changes
  fn language(&self, cx: &Cx) -> String {
    track(&self.settings);
    self.settings.read(cx).language.clone()
  }
}

/// The shell's language, e.g. `de` or `pt-BR`.
#[specta]
fn language(cx: Cx, i18n: Data<I18n>) -> String {
  i18n.language(&cx)
}

/// The translation of `key` (`a.b` for nested keys) in the shell's language, then its
/// prefix (`pt` for `pt-BR`), then English; the key itself when there is none. `%{name}`
/// is replaced with `vars.name`.
#[specta]
fn t(cx: Cx, i18n: Data<I18n>, key: String, vars: Opt<Json<Map<String, Value>>>) -> String {
  let language = i18n.language(&cx);
  let mut cache = i18n.cache.borrow_mut();
  if cache.as_ref().is_none_or(|t| t.language != language) {
    *cache = Some(Translations::load(&i18n.dir, language));
  }
  let text = cache.as_ref().and_then(|t| t.get(&key)).unwrap_or(&key);
  substitute(text, &vars.0.map(|v| v.0).unwrap_or_default())
}

/// `corona/i18n`: the plugin's translations, from `locales/` in its directory. Every
/// plugin has it; `settings` follows the shell's language.
pub fn module(dir: &Path, settings: Entity<Settings>, m: &mut ModuleBuilder) {
  m.provide(I18n {
    dir: dir.join("locales"),
    cache: RefCell::default(),
    settings,
  });
  host_module!(m, { sync: [language, t] });
}

#[cfg(test)]
mod tests {
  use std::fs;

  use serde_json::json;

  use super::*;

  #[test]
  fn falls_back_to_the_prefix_then_english_then_the_key() {
    let dir = tempfile::tempdir().unwrap();
    let locales = dir.path().join("locales");
    fs::create_dir(&locales).unwrap();
    fs::write(
      locales.join("app.yml"),
      r#"
_version: 2
a:
  pt-BR: br
  pt: pt
b:
  pt: pt b
c:
  d:
    en: "nested %{n}"
"#,
    )
    .unwrap();
    // one file per language works too
    fs::write(locales.join("en.yml"), "e: english\n").unwrap();

    let t = Translations::load(&locales, "pt-BR".into());
    assert_eq!(t.get("a"), Some("br"));
    assert_eq!(t.get("b"), Some("pt b"));
    assert_eq!(t.get("c.d"), Some("nested %{n}"));
    assert_eq!(t.get("e"), Some("english"));
    assert_eq!(t.get("c"), None);
    assert_eq!(t.get("x"), None);

    let vars = json!({ "n": 3, "s": "x" });
    assert_eq!(
      substitute("%{n} %{s} %{missing}", vars.as_object().unwrap()),
      "3 x %{missing}"
    );
  }

  #[test]
  fn files_may_not_link_out_of_the_plugin() {
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("pt.yml"), "a: outside\n").unwrap();
    let plugin = tempfile::tempdir().unwrap();
    let locales = plugin.path().join("locales");
    fs::create_dir(&locales).unwrap();
    fs::write(plugin.path().join("en.yml"), "a: inside\n").unwrap();
    std::os::unix::fs::symlink("../en.yml", locales.join("en.yml")).unwrap();
    std::os::unix::fs::symlink(outside.path().join("pt.yml"), locales.join("pt.yml")).unwrap();

    assert_eq!(
      Translations::load(&locales, "en".into()).get("a"),
      Some("inside")
    );
    // the English one, not the linked pt.yml
    assert_eq!(
      Translations::load(&locales, "pt".into()).get("a"),
      Some("inside")
    );
  }

  #[test]
  fn values_are_not_substituted_again() {
    // a user's name of `%{b}` stays as it is, whatever order the vars are in
    let vars = json!({ "a": "%{b}", "b": "x", "n": null });
    assert_eq!(
      substitute("%{a} %{b} %%{b}} %{n} %{é {x}", vars.as_object().unwrap()),
      "%{b} x %x} null %{é {x}"
    );
  }
}
