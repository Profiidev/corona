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
