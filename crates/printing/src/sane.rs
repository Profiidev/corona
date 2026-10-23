//! Scanners without eSCL, e.g. older Samsung multi-function printers, through
//! SANE's `scanimage` (sane-backends).

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use futures_lite::{AsyncBufReadExt, StreamExt, io::BufReader};
use smol::process::{Command, Stdio};

use crate::state::{
  ColorMode, Format, InputSource, ScanSettings, Scanner, ScannerCaps, SourceCaps,
};

const SCANIMAGE: &str = "scanimage";

async fn run(args: &[&str]) -> Result<String> {
  let out = Command::new(SCANIMAGE)
    .args(args)
    .output()
    .await
    .context("running scanimage, is sane-backends installed?")?;
  if !out.status.success() {
    bail!(
      "scanimage failed: {}",
      String::from_utf8_lossy(&out.stderr).trim()
    );
  }
  Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every scanner SANE knows, without the eSCL ones Avahi already finds.
pub async fn scanners() -> Result<Vec<Scanner>> {
  Ok(parse_list(&run(&["-f", "%d\t%v %m%n"]).await?))
}

pub fn parse_list(out: &str) -> Vec<Scanner> {
  let mut list: Vec<Scanner> = Vec::new();
  for line in out.lines() {
    let Some((device, name)) = line.split_once('\t') else {
      continue;
    };
    // sane-airscan and SANE's escl backend repeat what Avahi announces
    if device.starts_with("airscan:") || device.starts_with("escl:") {
      continue;
    }
    let mut name = name.trim().to_string();
    if list.iter().any(|s| s.name == name) {
      name = format!("{name} ({device})");
    }
    list.push(Scanner {
      name,
      model: Some(device.into()),
      address: String::new(),
      port: 0,
      root: String::new(),
      tls: false,
      sane: Some(device.into()),
    });
  }
  list
}

/// What the backend calls the choices of `ScanSettings`.
#[derive(Debug, Default, PartialEq)]
pub struct Options {
  modes: Vec<(ColorMode, String)>,
  sources: Vec<(InputSource, String)>,
  resolutions: Vec<u32>,
}

fn color_mode(value: &str) -> Option<ColorMode> {
  let value = value.to_lowercase();
  if value.contains("color") || value.contains("colour") {
    Some(ColorMode::Color)
  } else if value.contains("gray") || value.contains("grey") {
    Some(ColorMode::Grayscale)
  } else if value.contains("lineart") || value.contains("binary") {
    Some(ColorMode::Monochrome)
  } else {
    None
  }
}

fn source(value: &str) -> Option<InputSource> {
  let value = value.to_lowercase();
  if value.contains("flatbed") {
    Some(InputSource::Flatbed)
  } else if value.contains("duplex") {
    None
  } else if value.contains("adf") || value.contains("feeder") {
    Some(InputSource::Adf)
  } else {
    None
  }
}

/// The options of `scanimage -A`, lines like `--mode Lineart|Gray|Color [Color]`
/// or `--resolution 75..1200dpi (in steps of 1) [300]`.
pub fn parse_options(out: &str) -> Options {
  let mut options = Options::default();
  for line in out.lines() {
    let Some((name, rest)) = line.trim().split_once(' ') else {
      continue;
    };
    let values = rest.split(" [").next().unwrap_or_default();
    let values = values.split(" (").next().unwrap_or_default();
    let values: Vec<&str> = values.trim_end_matches("dpi").split('|').collect();
    match name {
      "--mode" => {
        for v in &values {
          if let Some(mode) = color_mode(v)
            && !options.modes.iter().any(|(m, _)| *m == mode)
          {
            options.modes.push((mode, v.to_string()));
          }
        }
        // color first, it is what a fresh scan picks
        options.modes.sort_by_key(|(m, _)| *m as u8);
      }
      "--source" => {
        for v in &values {
          if let Some(source) = source(v)
            && !options.sources.iter().any(|(s, _)| *s == source)
          {
            options.sources.push((source, v.to_string()));
          }
        }
      }
      "--resolution" => {
        options.resolutions = match values[0].split_once("..") {
          Some((lo, hi)) => {
            let (lo, hi) = (lo.parse().unwrap_or(0), hi.parse().unwrap_or(0));
            [75, 100, 150, 200, 300, 600, 1200]
              .into_iter()
              .filter(|dpi| (lo..=hi).contains(dpi))
              .collect()
          }
          None => values.iter().filter_map(|v| v.parse().ok()).collect(),
        }
      }
      _ => {}
    }
  }
  options
}

impl Options {
  fn caps(&self) -> ScannerCaps {
    let sources = match self.sources.is_empty() {
      true => vec![InputSource::Flatbed],
      false => self.sources.iter().map(|(s, _)| *s).collect(),
    };
    ScannerCaps {
      version: "SANE".into(),
      sources: sources
        .into_iter()
        // shortcut: no page size, only eSCL needs it, SANE scans the whole bed
        .map(|source| SourceCaps {
          source,
          max_width: 0,
          max_height: 0,
        })
        .collect(),
      color_modes: self.modes.iter().map(|(m, _)| *m).collect(),
      formats: vec![Format::Pdf, Format::Png, Format::Jpeg],
      resolutions: self.resolutions.clone(),
      state: String::new(),
    }
  }
}

async fn options(device: &str) -> Result<Options> {
  Ok(parse_options(&run(&["-d", device, "-A"]).await?))
}

pub async fn capabilities(device: &str) -> Result<ScannerCaps> {
  Ok(options(device).await?.caps())
}

/// Scans every page into `settings.destination` as `scan-<stamp>-<n>.<ext>`.
pub async fn scan(
  device: &str,
  settings: &ScanSettings,
  stamp: &str,
  mut on_page: impl FnMut(u32),
) -> Result<Vec<PathBuf>> {
  let options = options(device).await?;
  let format = match settings.format {
    Format::Pdf => "pdf",
    Format::Png => "png",
    Format::Jpeg => "jpeg",
  };
  let ext = settings.format.extension();
  // `--batch` is a printf format, a `%` in the folder must stay literal
  let dir = settings.destination.to_string_lossy().replace('%', "%%");
  let pattern = PathBuf::from(dir).join(format!("scan-{stamp}-%d.{ext}"));
  let first = settings.destination.join(format!("scan-{stamp}-1.{ext}"));
  let mut cmd = Command::new(SCANIMAGE);
  cmd
    .args(["-d", device, "--format", format])
    .arg(format!("--resolution={}", settings.resolution))
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  if let Some((_, mode)) = options
    .modes
    .iter()
    .find(|(m, _)| *m == settings.color_mode)
  {
    cmd.arg(format!("--mode={mode}"));
  }
  if let Some((_, source)) = options.sources.iter().find(|(s, _)| *s == settings.source) {
    cmd.arg(format!("--source={source}"));
  }
  // `--batch-count=1` would end the PDF without its trailer
  match settings.source {
    InputSource::Flatbed => cmd.arg("-o").arg(&first),
    InputSource::Adf => cmd
      .arg(format!("--batch={}", pattern.display()))
      .arg("--batch-print"),
  };

  let mut child = cmd
    .spawn()
    .context("running scanimage, is sane-backends installed?")?;
  let mut files = Vec::new();
  // `--batch-print` names each page's file once it is written
  let mut lines = BufReader::new(child.stdout.take().context("no scanimage output")?).lines();
  while let Some(line) = lines.next().await {
    files.push(PathBuf::from(line?));
    on_page(files.len() as u32);
  }
  let out = child.output().await?;
  // the feeder running empty after some pages ends the job, it is no failure
  if files.is_empty() && !out.status.success() {
    bail!(
      "scanimage failed: {}",
      String::from_utf8_lossy(&out.stderr).trim()
    );
  }
  // a flatbed page, or a PDF with every page in its first file, is never printed
  if files.is_empty() && first.exists() {
    files.push(first);
    on_page(1);
  }
  Ok(files)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lists_all_but_escl() {
    let list = parse_list(
      "xerox_mfp:tcp 192.168.1.50\tSamsung CLX-3180 Series\n\
       airscan:e0:Brother\tBrother MFC\n\
       xerox_mfp:libusb:001:004\tSamsung CLX-3180 Series\n",
    );
    let names: Vec<_> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
      names,
      [
        "Samsung CLX-3180 Series",
        "Samsung CLX-3180 Series (xerox_mfp:libusb:001:004)"
      ]
    );
    assert_eq!(list[0].sane.as_deref(), Some("xerox_mfp:tcp 192.168.1.50"));
  }

  #[test]
  fn reads_options() {
    let out = "All options specific to device `xerox_mfp:tcp 192.168.1.50':
  Standard:
    --resolution 75|100|150|200|300|600dpi [150]
    --mode Lineart|Halftone|Gray|Color [Color]
    --source Flatbed|ADF|Auto [Flatbed]
";
    let options = parse_options(out);
    assert_eq!(options.resolutions, [75, 100, 150, 200, 300, 600]);
    assert_eq!(
      options.modes,
      [
        (ColorMode::Color, "Color".into()),
        (ColorMode::Grayscale, "Gray".into()),
        (ColorMode::Monochrome, "Lineart".into()),
      ]
    );
    assert_eq!(
      options.sources,
      [
        (InputSource::Flatbed, "Flatbed".into()),
        (InputSource::Adf, "ADF".into()),
      ]
    );

    let out = "    --resolution 50..1200dpi (in steps of 1) [300]
    --source Flatbed|Automatic Document Feeder|ADF Duplex [Flatbed]
";
    let options = parse_options(out);
    assert_eq!(options.resolutions, [75, 100, 150, 200, 300, 600, 1200]);
    assert_eq!(
      options.sources,
      [
        (InputSource::Flatbed, "Flatbed".into()),
        (InputSource::Adf, "Automatic Document Feeder".into()),
      ]
    );
  }
}
