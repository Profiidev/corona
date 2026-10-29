//! Checks over the locale files every crate shares

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

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
          let text = std::fs::read_to_string(path).unwrap();
          // test fixtures, like the made-up widgets of surface's tests, have no texts
          let code = text.split("#[cfg(test)]").next().unwrap_or_default();
          out.push(code.to_string());
        }
      }
    }
    let mut files = Vec::new();
    // every crate shares these locale files
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for dir in std::fs::read_dir(crates).unwrap().flatten() {
      sources(&dir.path().join("src"), &mut files);
    }

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
    keys.extend(corona_udisks::ui::page::runtime_keys());
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

    let mut unused: Vec<_> = messages("en", "app.")
      .into_keys()
      .filter(|k| !keys.contains(k))
      .collect();
    unused.sort();
    assert!(unused.is_empty(), "never used: {unused:?}");

    for locale in corona_components::i18n::languages() {
      let known = messages(&locale, "app.");
      let mut missing: Vec<_> = keys.iter().filter(|k| !known.contains_key(*k)).collect();
      missing.sort();
      missing.dedup();
      assert!(missing.is_empty(), "missing in {locale}: {missing:?}");
    }
  }
}
