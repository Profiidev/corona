//! `corona/printing`: CUPS printers and jobs, and eSCL scanners.

use std::path::Path;

use anyhow::{Context as _, Result, ensure};
use corona_printing as pr;
use corona_printing::{Printing, PrintingExt};
use gpui_kit::App;
use gpui_shell::{Capabilities, HostModule};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Glob, Module},
  module::{Subscribe, Subscriptions, read},
};
use corona_macros::named;

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum PrinterState {
  Idle,
  Processing,
  Stopped,
}

#[derive(Serialize, TS)]
struct Marker {
  name: String,
  /// percent, null when the printer cannot tell
  level: Option<u8>,
  /// `#rrggbb`
  color: Option<String>,
  kind: String,
}

#[derive(Serialize, TS)]
struct Printer {
  name: String,
  info: String,
  location: String,
  state: PrinterState,
  reasons: Vec<String>,
  accepting: bool,
  markers: Vec<Marker>,
}

#[derive(Serialize, TS)]
#[serde(rename_all = "snake_case")]
enum JobState {
  Pending,
  Held,
  Processing,
  Stopped,
  Canceled,
  Aborted,
  Completed,
}

#[derive(Serialize, TS)]
struct Job {
  id: i32,
  name: String,
  printer: String,
  state: JobState,
  /// not finished yet, so it can be canceled
  active: bool,
  pages_done: Option<i32>,
  pages_total: Option<i32>,
}

#[derive(Serialize, TS)]
struct Scanner {
  name: String,
  model: Option<String>,
  /// false when it only offers eSCL over TLS, which corona cannot scan with
  supported: bool,
}

#[derive(Serialize, TS)]
struct NetworkPrinter {
  name: String,
  model: Option<String>,
  address: String,
}

#[derive(Serialize, TS)]
struct ScanProgress {
  scanner: String,
  pages: u32,
}

#[derive(Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
enum ColorMode {
  Color,
  Grayscale,
  Monochrome,
}

#[derive(Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
enum Source {
  Flatbed,
  Adf,
}

#[derive(Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
enum Format {
  Pdf,
  Png,
  Jpeg,
}

#[derive(Serialize, TS)]
struct ScannerCapabilities {
  sources: Vec<Source>,
  color_modes: Vec<ColorMode>,
  formats: Vec<Format>,
  resolutions: Vec<u32>,
  /// the scanner's own state, e.g. `Idle`
  state: String,
}

#[derive(Deserialize, TS)]
struct ScanRequest {
  /// dpi, one of `resolutions`
  resolution: u32,
  color_mode: ColorMode,
  source: Source,
  format: Format,
}

impl From<&pr::Printer> for Printer {
  fn from(p: &pr::Printer) -> Self {
    Self {
      name: p.name.clone(),
      info: p.info.clone(),
      location: p.location.clone(),
      state: match p.state {
        pr::PrinterState::Idle => PrinterState::Idle,
        pr::PrinterState::Processing => PrinterState::Processing,
        pr::PrinterState::Stopped => PrinterState::Stopped,
      },
      reasons: p.reasons.clone(),
      accepting: p.accepting,
      markers: p
        .markers
        .iter()
        .map(|m| Marker {
          name: m.name.clone(),
          level: m.level,
          color: m.color.map(|c| format!("#{c:06x}")),
          kind: m.kind.clone(),
        })
        .collect(),
    }
  }
}

impl From<&pr::Job> for Job {
  fn from(j: &pr::Job) -> Self {
    Self {
      id: j.id,
      name: j.name.clone(),
      printer: j.printer.clone(),
      state: match j.state {
        pr::JobState::Pending => JobState::Pending,
        pr::JobState::Held => JobState::Held,
        pr::JobState::Processing => JobState::Processing,
        pr::JobState::Stopped => JobState::Stopped,
        pr::JobState::Canceled => JobState::Canceled,
        pr::JobState::Aborted => JobState::Aborted,
        pr::JobState::Completed => JobState::Completed,
      },
      active: j.state.is_active(),
      pages_done: j.pages_done,
      pages_total: j.pages_total,
    }
  }
}

impl From<&pr::Scanner> for Scanner {
  fn from(s: &pr::Scanner) -> Self {
    Self {
      name: s.name.clone(),
      model: s.model.clone(),
      supported: !s.tls_only,
    }
  }
}

impl From<pr::ScannerCaps> for ScannerCapabilities {
  fn from(caps: pr::ScannerCaps) -> Self {
    Self {
      sources: caps
        .sources
        .iter()
        .map(|s| match s.source {
          pr::InputSource::Flatbed => Source::Flatbed,
          pr::InputSource::Adf => Source::Adf,
        })
        .collect(),
      color_modes: caps
        .color_modes
        .iter()
        .map(|m| match m {
          pr::ColorMode::Color => ColorMode::Color,
          pr::ColorMode::Grayscale => ColorMode::Grayscale,
          pr::ColorMode::Monochrome => ColorMode::Monochrome,
        })
        .collect(),
      formats: caps
        .formats
        .iter()
        .map(|f| match f {
          pr::Format::Pdf => Format::Pdf,
          pr::Format::Png => Format::Png,
          pr::Format::Jpeg => Format::Jpeg,
        })
        .collect(),
      resolutions: caps.resolutions,
      state: caps.state,
    }
  }
}

impl ScanRequest {
  /// Plugins cannot write files, so scans always go to the pictures folder.
  fn settings(&self) -> pr::ScanSettings {
    pr::ScanSettings {
      resolution: self.resolution,
      color_mode: match self.color_mode {
        ColorMode::Color => pr::ColorMode::Color,
        ColorMode::Grayscale => pr::ColorMode::Grayscale,
        ColorMode::Monochrome => pr::ColorMode::Monochrome,
      },
      source: match self.source {
        Source::Flatbed => pr::InputSource::Flatbed,
        Source::Adf => pr::InputSource::Adf,
      },
      format: match self.format {
        Format::Pdf => pr::Format::Pdf,
        Format::Png => pr::Format::Png,
        Format::Jpeg => pr::Format::Jpeg,
      },
      destination: pr::pictures_dir(),
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Printers,
  Jobs,
  Scanners,
  NetworkPrinters,
  Scan,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Printing(value)
  }
}

/// `path` canonical, when the plugin may read it.
fn readable(capabilities: &Capabilities, path: &str) -> Result<std::path::PathBuf> {
  let path = Path::new(path);
  ensure!(path.is_absolute(), "`{}` is not absolute", path.display());
  let path = std::fs::canonicalize(path).with_context(|| path.display().to_string())?;
  ensure!(
    capabilities.may_read(&path),
    "`{}` is not readable",
    path.display()
  );
  Ok(path)
}

pub fn module(
  capabilities: Capabilities,
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
) -> HostModule {
  let printable = capabilities.clone();
  Module::new("corona/printing")
    .func(read(
      reads,
      subs,
      "listPrinters",
      Updates::Printers,
      |cx| cx.printing().printers.clone(),
      |cx| {
        let printers = cx.printing().list_printers(cx);
        printers.iter().map(Printer::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "listJobs",
      Updates::Jobs,
      |cx| cx.printing().jobs.clone(),
      |cx| {
        let jobs = cx.printing().list_jobs(cx);
        jobs.iter().map(Job::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "listScanners",
      Updates::Scanners,
      |cx| cx.printing().scanners.clone(),
      |cx| {
        let scanners = cx.printing().list_scanners(cx);
        scanners.iter().map(Scanner::from).collect::<Vec<_>>()
      },
    ))
    .func(read(
      reads,
      subs,
      "listNetworkPrinters",
      Updates::NetworkPrinters,
      |cx| cx.printing().network_printers.clone(),
      {
        let reads = reads.clone();
        move |cx| {
          // which are new also depends on the queues
          reads.record(Updates::Printers.into());
          let found = cx.printing().new_printers(cx);
          found
            .into_iter()
            .map(|p| NetworkPrinter {
              name: p.name.clone(),
              model: p.model.clone(),
              address: p.address.clone(),
            })
            .collect::<Vec<_>>()
        }
      },
    ))
    .func(read(
      reads,
      subs,
      "scanProgress",
      Updates::Scan,
      |cx| cx.printing().scan.clone(),
      |cx| {
        cx.printing().scan_progress(cx).map(|p| ScanProgress {
          scanner: p.scanner.clone(),
          pages: p.pages,
        })
      },
    ))
    .func(named!("cancelJob", |pr: Glob<Printing>, id: i32| pr.cancel_job(id)))
    .func(named!(
      "pausePrinter",
      |pr: Glob<Printing>, printer: String| pr.set_paused(&printer, true)
    ))
    .func(named!(
      "resumePrinter",
      |pr: Glob<Printing>, printer: String| pr.set_paused(&printer, false)
    ))
    .func(named!(
      "addPrinter",
      /// Adds a queue for a printer from `listNetworkPrinters`; CUPS may ask
      /// the user for an administrator password.
      |cx: Cx, pr: Glob<Printing>, name: String| pr.add_printer(&name, &cx)
    ))
    .func(named!(
      "printFile",
      /// Prints a file the plugin may read, e.g. one from `pickFiles`.
      move |pr: Glob<Printing>, printer: String, path: String| {
        let path = readable(&printable, &path);
        let printing = pr.clone();
        async move { printing.print_file(&printer, path?).await }
      }
    ))
    .func(named!(
      "scannerCapabilities",
      |cx: Cx, pr: Glob<Printing>, scanner: String| {
        let caps = pr.scanner_capabilities(&scanner, &cx);
        async move { caps.await.map(ScannerCapabilities::from) }
      }
    ))
    .func(named!(
      "scan",
      /// Scans into the user's pictures folder and returns the saved files,
      /// which the plugin may then read.
      move |cx: &mut App, scanner: String, request: ScanRequest| {
        let task = cx.printing().clone().scan(&scanner, request.settings(), cx);
        let capabilities = capabilities.clone();
        async move {
          let files = task.await?;
          Ok::<_, anyhow::Error>(
            files
              .into_iter()
              .map(|file| {
                let shown = file.to_string_lossy().into_owned();
                capabilities.grant_read(file);
                shown
              })
              .collect::<Vec<_>>(),
          )
        }
      }
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn json(value: impl Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
  }

  #[test]
  fn printers_and_jobs() {
    let printer = pr::Printer {
      name: "Office".into(),
      info: String::new(),
      location: String::new(),
      device_uri: String::new(),
      state: pr::PrinterState::Stopped,
      reasons: vec!["paused".into()],
      accepting: false,
      markers: vec![pr::Marker {
        name: "Cyan".into(),
        level: None,
        color: Some(0x00ffff),
        kind: "ink".into(),
      }],
    };
    let converted = json(Printer::from(&printer));
    assert_eq!(converted["state"], "stopped");
    assert_eq!(converted["markers"][0]["color"], "#00ffff");
    assert!(converted["markers"][0]["level"].is_null());

    let job = pr::Job {
      id: 3,
      name: "doc".into(),
      printer: "Office".into(),
      state: pr::JobState::Held,
      pages_done: Some(1),
      pages_total: None,
      created: Some(1),
    };
    let converted = json(Job::from(&job));
    assert_eq!(converted["state"], "held");
    assert_eq!(converted["active"], true);
    assert_eq!(converted["pages_done"], 1);
  }

  #[test]
  fn scan_requests_go_to_pictures() {
    let request: ScanRequest = serde_json::from_value(serde_json::json!({
      "resolution": 300,
      "color_mode": "grayscale",
      "source": "adf",
      "format": "jpeg",
    }))
    .unwrap();
    let settings = request.settings();
    assert_eq!(settings.color_mode, pr::ColorMode::Grayscale);
    assert_eq!(settings.source, pr::InputSource::Adf);
    assert_eq!(settings.destination, pr::pictures_dir());
  }

  #[test]
  fn unreadable_paths_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("doc.txt");
    std::fs::write(&file, "x").unwrap();
    let capabilities = Capabilities::default();
    let path = file.to_string_lossy();
    assert!(readable(&capabilities, &path).is_err());
    assert!(readable(&capabilities, "relative.txt").is_err());
    capabilities.grant_read(dir.path().canonicalize().unwrap());
    assert!(readable(&capabilities, &path).is_ok());
  }
}
