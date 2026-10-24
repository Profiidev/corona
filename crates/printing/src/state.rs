use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrinterState {
  Idle,
  Processing,
  Stopped,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
  pub name: String,
  /// percent, `None` when the printer cannot tell
  pub level: Option<u8>,
  /// `0xRRGGBB`
  pub color: Option<u32>,
  pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Printer {
  pub name: String,
  pub info: String,
  pub location: String,
  /// where CUPS sends the jobs, e.g. `ipp://10.0.0.7/ipp/print`
  pub device_uri: String,
  pub state: PrinterState,
  pub reasons: Vec<String>,
  pub accepting: bool,
  pub markers: Vec<Marker>,
  /// the device did not answer a connect, CUPS only notices on the next job
  pub offline: bool,
}

/// An IPP printer on the network that CUPS has no queue for yet.
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkPrinter {
  /// the DNS-SD service name, unique per printer
  pub name: String,
  pub model: Option<String>,
  pub address: String,
  /// a `dnssd://` URI, which CUPS resolves itself on every job
  pub uri: String,
  /// the same printer by its address now, for a cupsd that cannot resolve
  /// `.local` names
  pub direct_uri: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
  Pending,
  Held,
  Processing,
  Stopped,
  Canceled,
  Aborted,
  Completed,
}

impl JobState {
  pub(crate) fn from_ipp(value: i32) -> Self {
    match value {
      3 => Self::Pending,
      4 => Self::Held,
      5 => Self::Processing,
      6 => Self::Stopped,
      7 => Self::Canceled,
      8 => Self::Aborted,
      _ => Self::Completed,
    }
  }

  pub fn is_active(self) -> bool {
    matches!(
      self,
      Self::Pending | Self::Held | Self::Processing | Self::Stopped
    )
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Job {
  pub id: i32,
  pub name: String,
  pub printer: String,
  pub state: JobState,
  pub pages_done: Option<i32>,
  pub pages_total: Option<i32>,
  /// unix seconds
  pub created: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scanner {
  /// the DNS-SD service name, unique per scanner
  pub name: String,
  pub model: Option<String>,
  pub address: String,
  pub port: u16,
  /// eSCL root path from the `rs` TXT record, without slashes
  pub root: String,
  /// only announced as `_uscans._tcp`, so it speaks HTTPS
  pub tls: bool,
  /// the SANE device, for a scanner without eSCL
  pub sane: Option<String>,
}

impl Scanner {
  pub fn base_url(&self) -> String {
    let host = if self.address.contains(':') {
      format!("[{}]", self.address)
    } else {
      self.address.clone()
    };
    let scheme = if self.tls { "https" } else { "http" };
    format!("{scheme}://{host}:{}/{}", self.port, self.root)
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputSource {
  Flatbed,
  Adf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorMode {
  Color,
  Grayscale,
  Monochrome,
}

impl ColorMode {
  pub(crate) fn escl(self) -> &'static str {
    match self {
      Self::Color => "RGB24",
      Self::Grayscale => "Grayscale8",
      Self::Monochrome => "BlackAndWhite1",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
  Pdf,
  Png,
  Jpeg,
}

impl Format {
  pub fn mime(self) -> &'static str {
    match self {
      Self::Pdf => "application/pdf",
      Self::Png => "image/png",
      Self::Jpeg => "image/jpeg",
    }
  }

  pub fn extension(self) -> &'static str {
    match self {
      Self::Pdf => "pdf",
      Self::Png => "png",
      Self::Jpeg => "jpg",
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceCaps {
  pub source: InputSource,
  /// in 1/300 inch
  pub max_width: u32,
  pub max_height: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScannerCaps {
  pub version: String,
  pub sources: Vec<SourceCaps>,
  pub color_modes: Vec<ColorMode>,
  pub formats: Vec<Format>,
  pub resolutions: Vec<u32>,
  /// `pwg:State` of `ScannerStatus`, e.g. `Idle`
  pub state: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScanSettings {
  pub resolution: u32,
  pub color_mode: ColorMode,
  pub source: InputSource,
  pub format: Format,
  pub destination: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScanProgress {
  pub scanner: String,
  pub pages: u32,
}
