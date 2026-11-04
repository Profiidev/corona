use std::{ffi::OsString, os::unix::ffi::OsStringExt, path::PathBuf};

/// The path of a `file://` uri, `None` for any other text
pub fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
  let rest = uri.strip_prefix("file://")?;
  let path = if rest.starts_with('/') {
    rest
  } else {
    rest
      .strip_prefix("localhost")
      .filter(|r| r.starts_with('/'))?
  };
  Some(PathBuf::from(OsString::from_vec(percent_decode(path))))
}

/// `%20` and friends as their bytes; a stray `%` stays as it is
pub fn percent_decode(text: &str) -> Vec<u8> {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    // `from_str_radix` alone would take `%+1`
    let hex = match bytes.get(i + 1..i + 3) {
      Some(&[a, b]) if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() => {
        (char::from(a).to_digit(16).zip(char::from(b).to_digit(16)))
          .map(|(a, b)| (a * 16 + b) as u8)
      }
      _ => None,
    };
    match (bytes[i], hex) {
      (b'%', Some(byte)) => {
        out.push(byte);
        i += 3;
      }
      (byte, _) => {
        out.push(byte);
        i += 1;
      }
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decodes() {
    assert_eq!(percent_decode("%ff"), [0xff]);
    // a sign is no hex digit
    assert_eq!(percent_decode("%+1"), b"%+1");
    assert_eq!(percent_decode("/100%/%zz%2"), b"/100%/%zz%2");
    assert_eq!(
      file_uri_to_path("file://localhost/a%20b.png"),
      Some("/a b.png".into())
    );
    assert_eq!(file_uri_to_path("/a.png"), None);
    assert_eq!(file_uri_to_path("file://host/x"), None);
    assert_eq!(file_uri_to_path("file://localhostfoo/x"), None);
    assert_eq!(file_uri_to_path("file:///x"), Some("/x".into()));
  }
}
