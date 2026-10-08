use chardetng::{EncodingDetector, Utf8Detection};

/// https://github.com/hyprland-community/hyprland-rs/blob/master/src/encoding.rs
pub fn decode_ipc_response(bytes: &[u8]) -> String {
  if let Ok(s) = std::str::from_utf8(bytes) {
    return s.to_owned();
  }

  let mut det = EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
  det.feed(bytes, true);
  let enc = det.guess(None, Utf8Detection::Allow);

  let (decoded, _, _) = enc.decode(bytes);
  if decoded.contains('\u{FFFD}') {
    return String::from_utf8_lossy(bytes).into_owned();
  }
  decoded.into_owned()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decodes() {
    assert_eq!(decode_ipc_response(b""), "");
    assert_eq!(decode_ipc_response("Fünf ✓ 🦀".as_bytes()), "Fünf ✓ 🦀");
    // window titles from apps writing Latin-1
    assert_eq!(
      decode_ipc_response(b"caf\xe9 cr\xe8me br\xfbl\xe9e"),
      "café crème brûlée"
    );
    // nothing decodes it cleanly: replacement characters, never a panic
    let junk = decode_ipc_response(b"\xff\xfe\x00\x81");
    assert!(!junk.is_empty());
  }
}
