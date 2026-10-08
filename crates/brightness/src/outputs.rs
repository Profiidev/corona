use std::{fs, path::Path};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Output {
  pub name: String,
  pub model: Option<String>,
}

pub(crate) fn connected(drm_dir: &Path) -> Vec<Output> {
  let mut outputs: Vec<Output> = fs::read_dir(drm_dir)
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(|connector| {
      let path = connector.path();
      let status = fs::read_to_string(path.join("status")).ok()?;
      if status.trim() != "connected" {
        return None;
      }
      let name = connector.file_name().into_string().ok()?;
      Some(Output {
        name: name.split_once('-')?.1.to_string(),
        model: fs::read(path.join("edid"))
          .ok()
          .and_then(|edid| edid_name(&edid)),
      })
    })
    .collect();
  outputs.sort_by(|a, b| a.name.cmp(&b.name));
  outputs
}

fn edid_name(edid: &[u8]) -> Option<String> {
  // four 18 byte descriptors start at byte 54
  (0..4).find_map(|i| {
    let descriptor = edid.get(54 + i * 18..72 + i * 18)?;
    if descriptor[..3] != [0, 0, 0] || descriptor[3] != 0xFC {
      return None;
    }
    let text = &descriptor[5..];
    let end = text.iter().position(|&b| b == b'\n').unwrap_or(text.len());
    let name = String::from_utf8_lossy(&text[..end]).trim().to_string();
    (!name.is_empty()).then_some(name)
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn edid_name() {
    let mut edid = vec![0u8; 128];
    // a serial number descriptor first, then the name
    edid[54..59].copy_from_slice(&[0, 0, 0, 0xFF, 0]);
    edid[72..77].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
    edid[77..90].copy_from_slice(b"DELL U2720Q\n ");
    assert_eq!(super::edid_name(&edid).as_deref(), Some("DELL U2720Q"));
    assert_eq!(super::edid_name(&[0; 128]), None);
  }

  #[test]
  fn edid_name_edges() {
    let descriptor = |edid: &mut Vec<u8>, i: usize, kind: u8, text: &[u8]| {
      let at = 54 + i * 18;
      edid[at..at + 5].copy_from_slice(&[0, 0, 0, kind, 0]);
      edid[at + 5..at + 5 + text.len()].copy_from_slice(text);
    };
    // the name in the last descriptor, without a newline
    let mut edid = vec![0u8; 128];
    descriptor(&mut edid, 3, 0xFC, b"ABCDEFGHIJKLM");
    assert_eq!(super::edid_name(&edid).as_deref(), Some("ABCDEFGHIJKLM"));
    // blank names do not count
    let mut blank = vec![0u8; 128];
    descriptor(&mut blank, 0, 0xFC, b"   \n");
    assert_eq!(super::edid_name(&blank), None);
    // a timing descriptor (non-zero first bytes) is skipped
    let mut timing = vec![0u8; 128];
    timing[54] = 1;
    timing[57] = 0xFC;
    timing[59..62].copy_from_slice(b"ABC");
    assert_eq!(super::edid_name(&timing), None);
    // invalid UTF-8 is replaced, not dropped
    let mut odd = vec![0u8; 128];
    descriptor(&mut odd, 1, 0xFC, b"A\xffB\n");
    assert_eq!(super::edid_name(&odd).as_deref(), Some("A\u{fffd}B"));
    // too short for any descriptor
    assert_eq!(super::edid_name(&edid[..71]), None);
    assert_eq!(super::edid_name(&[]), None);
    // long enough for the first descriptor only
    let mut first = vec![0u8; 72];
    descriptor(&mut first, 0, 0xFC, b"X\n");
    assert_eq!(super::edid_name(&first).as_deref(), Some("X"));
  }

  #[test]
  fn connected_outputs() {
    let root = tempfile::tempdir().unwrap();
    let write = |file: &str, value: &[u8]| {
      let path = root.path().join(file);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, value).unwrap();
    };
    let mut edid = vec![0u8; 128];
    edid[54..59].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
    edid[59..63].copy_from_slice(b"LG\n ");
    write("card1-HDMI-A-1/status", b"connected\n");
    write("card1-HDMI-A-1/edid", &edid);
    write("card1-DP-1/status", b"connected");
    write("card1-DP-2/status", b"disconnected\n");
    write("card1-DP-3/status", b"unknown\n");
    write("card1/status", b"connected");
    fs::create_dir_all(root.path().join("card1-DP-4")).unwrap();
    write("version", b"1");
    assert_eq!(
      connected(root.path()),
      [
        Output {
          name: "DP-1".into(),
          model: None
        },
        Output {
          name: "HDMI-A-1".into(),
          model: Some("LG".into())
        },
      ]
    );
    assert!(connected(&root.path().join("missing")).is_empty());
  }
}
