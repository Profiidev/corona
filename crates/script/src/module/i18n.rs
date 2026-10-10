use std::{
  cell::RefCell,
  collections::BTreeMap,
  path::{Path, PathBuf},
  rc::Rc,
};

use corona_config::Config;
use corona_macros::named;
use corona_utils::error::ErrorLogExt;
use gpui_shell::HostModule;
use rust_i18n_support::try_load_locales;
use serde_json::{Map, Value};

use crate::{host_fn::Module, module::Subscribe};

/// The fallback when neither the language nor its prefix has a translation
const FALLBACK: &str = "en";

/// The shell's language, e.g. `de` or `pt-BR`
fn language() -> String {
  rust_i18n::locale().to_string()
}

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

/// `corona/i18n`: the plugin's translations, from `locales/` in its directory.
/// Every plugin has it.
pub fn module(dir: &Path, subs: &mut Vec<Subscribe>) -> HostModule {
  let dir: PathBuf = dir.join("locales");
  // ponytail: read once per language and view, edits show after a reload of the plugin
  let cache: Rc<RefCell<Option<Translations>>> = Rc::default();
  subs.push(watch());

  Module::new("corona/i18n")
    .func(named!(
      "language",
      /// The shell's language, e.g. `de` or `pt-BR`.
      || rust_i18n::locale().to_string()
    ))
    .func(named!(
      "t",
      /// The translation of `key` (`a.b` for nested keys) in the shell's
      /// language, then its prefix (`pt` for `pt-BR`), then English; the key
      /// itself when there is none. `%{name}` is replaced with `vars.name`.
      move |key: String, vars: Option<Map<String, Value>>| {
        let language = language();
        let mut cache = cache.borrow_mut();
        if cache.as_ref().is_none_or(|t| t.language != language) {
          *cache = Some(Translations::load(&dir, language));
        }
        let text = cache.as_ref().and_then(|t| t.get(&key)).unwrap_or(&key);
        substitute(text, &vars.unwrap_or_default())
      }
    ))
    .into()
}

/// Renders the script again when the language changes
fn watch() -> Subscribe {
  Box::new(move |runtime, root, cx| {
    let (runtime, root) = (runtime.clone(), root.clone());
    let mut last = language();
    cx.observe_global::<Config>(move |cx| {
      let now = language();
      if now != last {
        last = now;
        runtime.refresh(&root, cx).log_err().ok();
      }
    })
  })
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
