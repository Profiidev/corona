/// `pattern` as a case-insensitive regex; a bad one is logged and matches nothing
pub fn filter_regex(pattern: &str) -> Option<regex::Regex> {
  if pattern.is_empty() {
    return None;
  }
  regex::RegexBuilder::new(pattern)
    .case_insensitive(true)
    .build()
    .inspect_err(|e| tracing::warn!("bad filter regex {pattern:?}: {e}"))
    .ok()
}

#[cfg(test)]
mod tests {
  use super::filter_regex;

  #[test]
  fn filters() {
    let re = filter_regex("noctalia|obs").unwrap();
    assert!(re.is_match("Noctalia Shell"));
    assert!(re.is_match("com.obsproject.Studio"));
    assert!(!re.is_match("firefox"));
    // nothing set hides nothing, and neither does a broken pattern
    assert!(filter_regex("").is_none());
    assert!(filter_regex("(").is_none());
  }
}
