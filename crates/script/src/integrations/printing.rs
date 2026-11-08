//! `corona/printing`: CUPS printers and jobs, and eSCL scanners.

use std::path::Path;

use anyhow::{Context as _, Result, ensure};
use corona_js::{
  Capabilities, Cx, Glob, Json, ModuleBuilder, Plugin, PluginRef, ScriptModule, Throws,
  host_module, json, track,
};
use corona_printing as pr;
use corona_printing::PrintingExt;
use gpui_kit::App;
use serde::{Deserialize, Serialize};
use specta::{Type, specta};

#[derive(Serialize, Type)]
#[serde(rename_all = "snake_case")]
enum PrinterState {
  Idle,
  Processing,
  Stopped,
}

#[derive(Serialize, Type)]
struct Marker {
  name: String,
  /// percent, null when the printer cannot tell
  level: Option<u8>,
  /// `#rrggbb`
  color: Option<String>,
  kind: String,
}

#[derive(Serialize, Type)]
struct Printer {
  name: String,
  info: String,
  location: String,
  state: PrinterState,
  reasons: Vec<String>,
  accepting: bool,
  /// the device did not answer, though CUPS may still call it idle
  offline: bool,
  markers: Vec<Marker>,
}

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
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

#[derive(Serialize, Type)]
struct Scanner {
  name: String,
  model: Option<String>,
  /// talks HTTPS, its certificate pinned on first use
  tls: bool,
}

#[derive(Serialize, Type)]
struct NetworkPrinter {
  name: String,
  model: Option<String>,
  address: String,
}

#[derive(Serialize, Type)]
struct ScanProgress {
  scanner: String,
  pages: u32,
}

#[derive(Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum ColorMode {
  Color,
  Grayscale,
  Monochrome,
}

#[derive(Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum Source {
  Flatbed,
  Adf,
}

#[derive(Clone, Copy, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
enum Format {
  Pdf,
  Png,
  Jpeg,
}

#[derive(Serialize, Type)]
struct ScannerCapabilities {
  sources: Vec<Source>,
  color_modes: Vec<ColorMode>,
  formats: Vec<Format>,
  resolutions: Vec<u32>,
  /// the scanner's own state, e.g. `Idle`
  state: String,
}

#[derive(Deserialize, Type)]
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
      offline: p.offline,
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
      tls: s.tls,
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

/// Printing and managing printers are separate grants, a plugin listing printers needs none.
fn require(capabilities: &Capabilities, manage: bool) -> Result<()> {
  let (allowed, key) = match manage {
    true => (capabilities.may_manage_printing(), "manage"),
    false => (capabilities.may_print(), "print"),
  };
  ensure!(
    allowed,
    "printing is not granted; set capabilities.printing.{key} to true"
  );
  Ok(())
}

type Pr = Glob<pr::Printing>;

#[specta]
async fn cancel_job(pr: Pr, plugin: Plugin, id: i32) -> Throws<()> {
  if let Err(e) = require(&plugin.capabilities, true) {
    return Err(e).into();
  }
  pr.cancel_job(id).await.into()
}

#[specta]
async fn pause_printer(pr: Pr, plugin: Plugin, printer: String) -> Throws<()> {
  if let Err(e) = require(&plugin.capabilities, true) {
    return Err(e).into();
  }
  pr.set_paused(&printer, true).await.into()
}

#[specta]
async fn resume_printer(pr: Pr, plugin: Plugin, printer: String) -> Throws<()> {
  if let Err(e) = require(&plugin.capabilities, true) {
    return Err(e).into();
  }
  pr.set_paused(&printer, false).await.into()
}

/// Adds a queue for a printer from `listNetworkPrinters`; CUPS may ask the user for an
/// administrator password.
#[specta]
async fn add_printer(cx: Cx, pr: Pr, plugin: Plugin, name: String) -> Throws<()> {
  if let Err(e) = require(&plugin.capabilities, true) {
    return Err(e).into();
  }
  let add = pr.add_printer(&name, &cx);
  add.await.into()
}

/// Prints a file the plugin may read, e.g. one from `pickFiles`.
#[specta]
async fn print_file(pr: Pr, plugin: Plugin, printer: String, path: String) -> Throws<()> {
  let path =
    require(&plugin.capabilities, false).and_then(|()| readable(&plugin.capabilities, &path));
  match path {
    Ok(path) => pr.print_file(&printer, path).await,
    Err(e) => Err(e),
  }
  .into()
}

#[specta]
async fn scanner_capabilities(
  cx: Cx,
  pr: Pr,
  plugin: Plugin,
  scanner: String,
) -> Throws<Json<ScannerCapabilities>> {
  if let Err(e) = require(&plugin.capabilities, true) {
    return Err(e).into();
  }
  let caps = pr.scanner_capabilities(&scanner, &cx);
  caps.await.map(|c| Json(c.into())).into()
}

/// Scans into the user's pictures folder and returns the saved files, which the plugin
/// may then read.
#[specta]
async fn scan(
  mut cx: Cx,
  plugin: Plugin,
  scanner: String,
  request: Json<ScanRequest>,
) -> Throws<Vec<String>> {
  if let Err(e) = require(&plugin.capabilities, true) {
    return Err(e).into();
  }
  let task = cx
    .printing()
    .clone()
    .scan(&scanner, request.0.settings(), &mut cx);
  let files = task.await.map(|files| {
    (files.into_iter())
      .map(|file| {
        let shown = file.to_string_lossy().into_owned();
        plugin.capabilities.grant_read(file);
        shown
      })
      .collect()
  });
  files.into()
}

/// `corona/printing`
pub struct PrintingModule;

impl ScriptModule for PrintingModule {
  const NAME: &str = "printing";
  const DOC: &str = "Printers, print jobs and document scanners.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listPrinters",
      "",
      |cx| cx.printing().printers.clone(),
      json(|cx| {
        let printers = cx.printing().list_printers(cx);
        printers.iter().map(Printer::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "listJobs",
      "",
      |cx| cx.printing().jobs.clone(),
      json(|cx| {
        let jobs = cx.printing().list_jobs(cx);
        jobs.iter().map(Job::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "listScanners",
      "",
      |cx| cx.printing().scanners.clone(),
      json(|cx| {
        let scanners = cx.printing().list_scanners(cx);
        scanners.iter().map(Scanner::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "listNetworkPrinters",
      "Printers found on the network that have no queue yet.",
      |cx| cx.printing().network_printers.clone(),
      json(|cx| {
        // which are new also depends on the queues
        track(&cx.printing().printers);
        let found = cx.printing().new_printers(cx);
        (found.into_iter())
          .map(|p| NetworkPrinter {
            name: p.name.clone(),
            model: p.model.clone(),
            address: p.address.clone(),
          })
          .collect::<Vec<_>>()
      }),
    );
    m.watch(
      "scanProgress",
      "The scan running; null when none is.",
      |cx| cx.printing().scan.clone(),
      json(|cx| {
        cx.printing().scan_progress(cx).map(|p| ScanProgress {
          scanner: p.scanner.clone(),
          pages: p.pages,
        })
      }),
    );
    host_module!(m, {
      async: [
        cancel_job,
        pause_printer,
        resume_printer,
        add_printer,
        print_file,
        scanner_capabilities,
        scan,
      ],
    });
  }
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
      offline: true,
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
  fn printing_needs_its_grants() {
    let none = Capabilities::default();
    let err = require(&none, true).unwrap_err().to_string();
    assert!(err.contains("capabilities.printing.manage"), "{err}");
    let err = require(&none, false).unwrap_err().to_string();
    assert!(err.contains("capabilities.printing.print"), "{err}");
    let manage = Capabilities::new().print_manage(true);
    assert!(require(&manage, true).is_ok());
    assert!(require(&manage, false).is_err());
    assert!(require(&Capabilities::new().print(true), false).is_ok());
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
