//! TypeScript sources, their types overwritten with whitespace so every line and column of
//! an error still points into the `.ts` file. Syntax that needs code generated (enums,
//! namespaces, parameter properties, decorators) is refused.

use std::{
  path::Path,
  sync::{Arc, Mutex},
};

use anyhow::{Result, anyhow};
use swc_common::{
  BytePos, GLOBALS, SourceMap,
  errors::{DiagnosticBuilder, Emitter, HANDLER, Handler},
  sync::Lrc,
};
use swc_ts_fast_strip::{ErrorCode, Mode, Options, operate};

pub(crate) fn is_ts(path: &Path) -> bool {
  matches!(
    path.extension().and_then(|e| e.to_str()),
    Some("ts" | "mts")
  )
}

/// Collects swc's diagnostics with their position
#[derive(Clone, Default)]
struct Diagnostics(Arc<Mutex<Vec<(BytePos, String)>>>);

impl Emitter for Diagnostics {
  fn emit(&mut self, diagnostic: &mut DiagnosticBuilder<'_>) {
    let at = diagnostic.span.primary_span().map_or(BytePos(0), |s| s.lo);
    self.0.lock().unwrap().push((at, diagnostic.message()));
  }
}

pub(crate) fn strip(path: &Path, source: &str) -> Result<String> {
  let map = Lrc::new(SourceMap::default());
  let diagnostics = Diagnostics::default();
  let handler = Handler::with_emitter(true, false, Box::new(diagnostics.clone()));
  let options = Options {
    module: Some(true),
    filename: Some(path.display().to_string()),
    mode: Mode::StripOnly,
    ..Default::default()
  };
  // swc drops a byte order mark, which would shift the first line
  let (bom, source) = match source.strip_prefix('\u{feff}') {
    Some(rest) => ("\u{feff}", rest),
    None => ("", source),
  };
  let stripped = GLOBALS.set(&Default::default(), || {
    HANDLER.set(&handler, || {
      operate(&map, &handler, source.to_string(), options)
    })
  });
  let error = match stripped {
    Ok(out) => return Ok(format!("{bom}{}", same_widths(source, out.code))),
    Err(error) => error,
  };
  let found = std::mem::take(&mut *diagnostics.0.lock().unwrap());
  let mut message = match found.is_empty() {
    true => format!("{}: {error}", path.display()),
    false => found
      .into_iter()
      .map(|(at, message)| {
        let at = map.lookup_char_pos(at);
        format!("{}:{}:{}: {message}", path.display(), at.line, at.col.0 + 1)
      })
      .collect::<Vec<_>>()
      .join("\n"),
  };
  if matches!(error.code, ErrorCode::UnsupportedSyntax) {
    message.push_str(
      "\nenums, namespaces, parameter properties and decorators need a transform; use erasable syntax",
    );
  }
  Err(anyhow!(message))
}

/// swc blanks an erased non-ASCII character with as many spaces as its UTF-16 width, while
/// QuickJS counts columns in bytes: pads each one back to its UTF-8 width.
fn same_widths(source: &str, stripped: String) -> String {
  if stripped.len() == source.len() {
    return stripped;
  }
  let mut out = String::with_capacity(source.len());
  let mut rest = stripped.as_str();
  for c in source.chars() {
    let width = match c.is_ascii() || rest.starts_with(c) {
      true => c.len_utf8(),
      false => c.len_utf16(),
    };
    // out of step: swc changed how it erases, positions drift but the code runs
    let Some((written, tail)) = rest.split_at_checked(width) else {
      return stripped;
    };
    out.push_str(written);
    out.extend(std::iter::repeat_n(' ', c.len_utf8() - written.len()));
    rest = tail;
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  fn strip_ts(source: &str) -> Result<String> {
    strip(Path::new("file.ts"), source)
  }

  #[test]
  fn types_become_whitespace_in_place() {
    let source = "interface A { x: number }\nconst label: string = 'é';\nlet n = (1 as number)!;\n";
    let out = strip_ts(source).unwrap();
    assert_eq!(out.len(), source.len());
    assert!(out.contains("const label         = 'é';"), "{out}");
    assert!(!out.contains("interface") && !out.contains(" as "), "{out}");
  }

  #[test]
  fn an_erased_non_ascii_type_keeps_its_byte_width() {
    let source = "type T = \"é𝒳\"; let a: \"é\" = \"é\"; const b = 1;\n";
    let out = strip_ts(source).unwrap();
    assert_eq!(out.find("const b"), source.find("const b"));
  }

  #[test]
  fn errors_name_line_and_column() {
    let error = strip_ts("let a = 1;\r\nlet b: = 2;\r\n")
      .unwrap_err()
      .to_string();
    assert!(error.contains("file.ts:2:"), "{error}");
    let error = strip_ts("\nenum Color { Red }\n").unwrap_err().to_string();
    assert!(
      error.contains("file.ts:2:1:") && error.contains("enums"),
      "{error}"
    );
    for source in [
      "namespace N { export const a = 1; }\n",
      "@dec class A {}\n",
      "class A { constructor(private a: number) {} }\n",
    ] {
      assert!(strip_ts(source).is_err(), "{source}");
    }
  }
}
