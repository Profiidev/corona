//! Scanning over eSCL (AirScan).

use std::{path::PathBuf, sync::Arc, time::Duration};

use crate::avahi::{SCANS, Service};
use crate::ipp::MAX_RESPONSE;
use crate::state::{
  ColorMode, Format, InputSource, ScanSettings, Scanner, ScannerCaps, SourceCaps,
};
use anyhow::{Context, Result, bail};
use futures_lite::AsyncReadExt;
use gpui_kit::http_client::{AsyncBody, HttpClient, Method, Request, Response};
use roxmltree::{Document, Node};

/// A scanner from its eSCL announcement.
pub fn scanner(service: &Service) -> Scanner {
  Scanner {
    name: service.name.clone(),
    model: service.record("ty").map(String::from),
    address: service.address.clone(),
    port: service.port,
    root: service
      .record("rs")
      .map(|rs| rs.trim_matches('/').to_string())
      .unwrap_or_else(|| "eSCL".into()),
    // only announced over TLS, see `tls::PinnedClient`
    tls: service.kind == SCANS,
    sane: None,
  }
}

async fn read(response: &mut Response<AsyncBody>) -> Result<Vec<u8>> {
  let mut body = Vec::new();
  let limit = MAX_RESPONSE as u64 + 1;
  response
    .body_mut()
    .take(limit)
    .read_to_end(&mut body)
    .await?;
  if body.len() > MAX_RESPONSE {
    bail!("the scanner answered with more than {MAX_RESPONSE} bytes");
  }
  Ok(body)
}

async fn get(client: &dyn HttpClient, url: &str) -> Result<(u16, Vec<u8>)> {
  let mut response = client.get(url, AsyncBody::empty(), false).await?;
  let body = read(&mut response).await?;
  Ok((response.status().as_u16(), body))
}

async fn get_text(client: &dyn HttpClient, url: &str) -> Result<String> {
  let (status, body) = get(client, url).await?;
  let body = String::from_utf8_lossy(&body).into_owned();
  if status != 200 {
    bail!("{url} answered {status}: {}", body.trim());
  }
  Ok(body)
}

pub async fn capabilities(client: &dyn HttpClient, scanner: &Scanner) -> Result<ScannerCaps> {
  let base = scanner.base_url();
  let xml = get_text(client, &format!("{base}/ScannerCapabilities")).await?;
  let mut caps = parse_capabilities(&xml)?;
  if let Ok(xml) = get_text(client, &format!("{base}/ScannerStatus")).await {
    caps.state = parse_status(&xml).unwrap_or_default();
  }
  Ok(caps)
}

fn named<'a, 'i>(node: Node<'a, 'i>, name: &str) -> impl Iterator<Item = Node<'a, 'i>> {
  let name = name.to_string();
  node
    .descendants()
    .filter(move |n| n.tag_name().name() == name)
}

fn text<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
  named(node, name).next()?.text().map(str::trim)
}

fn push_unique<T: PartialEq>(list: &mut Vec<T>, item: T) {
  if !list.contains(&item) {
    list.push(item);
  }
}

pub fn parse_capabilities(xml: &str) -> Result<ScannerCaps> {
  let doc = Document::parse(xml).context("parsing ScannerCapabilities")?;
  let root = doc.root_element();
  let mut caps = ScannerCaps {
    version: text(root, "Version").unwrap_or("2.0").to_string(),
    ..Default::default()
  };
  for (tag, source) in [("Platen", InputSource::Flatbed), ("Adf", InputSource::Adf)] {
    if let Some(node) = named(root, tag).next() {
      let size = |name| text(node, name).and_then(|t| t.parse().ok()).unwrap_or(0);
      caps.sources.push(SourceCaps {
        source,
        max_width: size("MaxWidth"),
        max_height: size("MaxHeight"),
      });
    }
  }
  for mode in named(root, "ColorMode").filter_map(|n| n.text()) {
    let mode = match mode.trim() {
      "RGB24" | "RGB48" => ColorMode::Color,
      "Grayscale8" | "Grayscale16" => ColorMode::Grayscale,
      "BlackAndWhite1" => ColorMode::Monochrome,
      _ => continue,
    };
    push_unique(&mut caps.color_modes, mode);
  }
  let formats = named(root, "DocumentFormat").chain(named(root, "DocumentFormatExt"));
  for format in formats.filter_map(|n| n.text()) {
    let format = match format.trim() {
      "application/pdf" => Format::Pdf,
      "image/png" => Format::Png,
      "image/jpeg" => Format::Jpeg,
      _ => continue,
    };
    push_unique(&mut caps.formats, format);
  }
  for resolution in named(root, "DiscreteResolution") {
    if let Some(dpi) = text(resolution, "XResolution").and_then(|t| t.parse().ok()) {
      push_unique(&mut caps.resolutions, dpi);
    }
  }
  caps.resolutions.sort_unstable();
  Ok(caps)
}

pub fn parse_status(xml: &str) -> Option<String> {
  let doc = Document::parse(xml).ok()?;
  text(doc.root_element(), "State").map(String::from)
}

pub fn scan_settings(caps: &ScannerCaps, settings: &ScanSettings) -> String {
  let source = caps.sources.iter().find(|s| s.source == settings.source);
  // the whole bed, in 1/300 inch; A4 when the scanner did not say
  let (width, height) = source
    .map(|s| (s.max_width, s.max_height))
    .filter(|(w, h)| *w > 0 && *h > 0)
    .unwrap_or((2480, 3508));
  let input = match settings.source {
    InputSource::Flatbed => "Platen",
    InputSource::Adf => "Feeder",
  };
  let (dpi, mime) = (settings.resolution, settings.format.mime());
  format!(
    r#"<?xml version="1.0" encoding="UTF-8"?>
<scan:ScanSettings xmlns:scan="http://schemas.hp.com/imaging/escl/2011/05/03" xmlns:pwg="http://www.pwg.org/schemas/2010/12/sm">
  <pwg:Version>{version}</pwg:Version>
  <pwg:ScanRegions>
    <pwg:ScanRegion>
      <pwg:ContentRegionUnits>escl:ThreeHundredthsOfInches</pwg:ContentRegionUnits>
      <pwg:XOffset>0</pwg:XOffset>
      <pwg:YOffset>0</pwg:YOffset>
      <pwg:Width>{width}</pwg:Width>
      <pwg:Height>{height}</pwg:Height>
    </pwg:ScanRegion>
  </pwg:ScanRegions>
  <pwg:InputSource>{input}</pwg:InputSource>
  <scan:ColorMode>{mode}</scan:ColorMode>
  <scan:XResolution>{dpi}</scan:XResolution>
  <scan:YResolution>{dpi}</scan:YResolution>
  <pwg:DocumentFormat>{mime}</pwg:DocumentFormat>
  <scan:DocumentFormatExt>{mime}</scan:DocumentFormatExt>
</scan:ScanSettings>
"#,
    version = caps.version,
    mode = settings.color_mode.escl(),
  )
}

/// Runs one scan job and saves every page it returns into
/// `settings.destination`. Calls `on_page` with the count after each page.
pub async fn scan(
  client: &Arc<dyn HttpClient>,
  scanner: &Scanner,
  caps: &ScannerCaps,
  settings: &ScanSettings,
  stamp: &str,
  mut on_page: impl FnMut(u32),
) -> Result<Vec<PathBuf>> {
  let base = scanner.base_url();
  let request = Request::builder()
    .method(Method::POST)
    .uri(format!("{base}/ScanJobs"))
    .header("Content-Type", "text/xml")
    .body(AsyncBody::from(scan_settings(caps, settings)))?;
  let mut response = client.send(request).await?;
  let body = read(&mut response).await?;
  if response.status().as_u16() != 201 {
    bail!(
      "scanner refused the job ({}): {}",
      response.status(),
      String::from_utf8_lossy(&body).trim()
    );
  }
  let location = response
    .headers()
    .get("Location")
    .and_then(|v| v.to_str().ok())
    .context("scan job without a Location")?;
  let job = if location.starts_with("http") {
    location.trim_end_matches('/').to_string()
  } else {
    let origin = base.trim_end_matches(&scanner.root).trim_end_matches('/');
    format!("{origin}/{}", location.trim_matches('/'))
  };

  let mut cancel = CancelJob {
    client: Some(client.clone()),
    job: job.clone(),
  };
  let pages = pages(&**client, &job, settings, stamp, &mut on_page).await;
  cancel.client = None;
  if pages.is_err() {
    delete(client.clone(), job).await;
  }
  pages
}

/// Scanners keep a broken job open and refuse new ones until it times out.
async fn delete(client: Arc<dyn HttpClient>, job: String) {
  let delete = Request::builder()
    .method(Method::DELETE)
    .uri(&job)
    .body(AsyncBody::empty());
  if let Ok(delete) = delete {
    client.send(delete).await.ok();
  }
}

/// Deletes the job when the scan is dropped before it finished.
struct CancelJob {
  client: Option<Arc<dyn HttpClient>>,
  job: String,
}

impl Drop for CancelJob {
  fn drop(&mut self) {
    if let Some(client) = self.client.take() {
      smol::spawn(delete(client, std::mem::take(&mut self.job))).detach();
    }
  }
}

async fn pages(
  client: &dyn HttpClient,
  job: &str,
  settings: &ScanSettings,
  stamp: &str,
  on_page: &mut impl FnMut(u32),
) -> Result<Vec<PathBuf>> {
  let ext = settings.format.extension();
  let mut files = Vec::new();
  let mut busy = 0;
  loop {
    let (status, body) = get(client, &format!("{job}/NextDocument")).await?;
    match status {
      200 => {
        // named like SANE's pages
        let page = files.len() + 1;
        let path = settings
          .destination
          .join(format!("scan-{stamp}-{page}.{ext}"));
        smol::fs::write(&path, body)
          .await
          .with_context(|| format!("saving {}", path.display()))?;
        files.push(path);
        on_page(files.len() as u32);
        // each page gets its own time to feed and scan
        busy = 0;
        // a flatbed has one page, asking again can error on some scanners
        if settings.source == InputSource::Flatbed {
          break;
        }
      }
      // the job has no more pages
      404 if !files.is_empty() => break,
      // still warming up or scanning
      503 if busy < 60 => {
        busy += 1;
        smol::Timer::after(Duration::from_millis(500)).await;
      }
      _ => bail!(
        "scanner answered {status}: {}",
        String::from_utf8_lossy(&body).trim()
      ),
    }
  }
  Ok(files)
}

#[cfg(test)]
mod tests {
  use std::sync::Mutex;

  use futures_lite::future::{block_on, poll_once};
  use gpui_kit::http_client::FakeHttpClient;

  use super::*;
  use crate::avahi::SCAN;

  const CAPS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<scan:ScannerCapabilities xmlns:scan="http://schemas.hp.com/imaging/escl/2011/05/03" xmlns:pwg="http://www.pwg.org/schemas/2010/12/sm">
  <pwg:Version>2.63</pwg:Version>
  <pwg:MakeAndModel>Test Scan 1</pwg:MakeAndModel>
  <scan:Platen><scan:PlatenInputCaps>
    <scan:MaxWidth>2550</scan:MaxWidth><scan:MaxHeight>3508</scan:MaxHeight>
    <scan:SettingProfiles><scan:SettingProfile>
      <scan:ColorModes><scan:ColorMode>RGB24</scan:ColorMode><scan:ColorMode>Grayscale8</scan:ColorMode><scan:ColorMode>Weird</scan:ColorMode></scan:ColorModes>
      <scan:DocumentFormats><pwg:DocumentFormat>image/jpeg</pwg:DocumentFormat><pwg:DocumentFormat>application/pdf</pwg:DocumentFormat><scan:DocumentFormatExt>image/jpeg</scan:DocumentFormatExt></scan:DocumentFormats>
      <scan:SupportedResolutions><scan:DiscreteResolutions>
        <scan:DiscreteResolution><scan:XResolution>600</scan:XResolution><scan:YResolution>600</scan:YResolution></scan:DiscreteResolution>
        <scan:DiscreteResolution><scan:XResolution>300</scan:XResolution><scan:YResolution>300</scan:YResolution></scan:DiscreteResolution>
      </scan:DiscreteResolutions></scan:SupportedResolutions>
    </scan:SettingProfile></scan:SettingProfiles>
  </scan:PlatenInputCaps></scan:Platen>
  <scan:Adf><scan:AdfSimplexInputCaps><scan:MaxWidth>2550</scan:MaxWidth><scan:MaxHeight>4200</scan:MaxHeight></scan:AdfSimplexInputCaps></scan:Adf>
</scan:ScannerCapabilities>"#;

  const STATUS: &str = r#"<?xml version="1.0"?>
<scan:ScannerStatus xmlns:scan="http://schemas.hp.com/imaging/escl/2011/05/03" xmlns:pwg="http://www.pwg.org/schemas/2010/12/sm">
  <pwg:Version>2.63</pwg:Version><pwg:State>Idle</pwg:State>
  <scan:Jobs><scan:JobInfo><pwg:JobState>Completed</pwg:JobState></scan:JobInfo></scan:Jobs>
</scan:ScannerStatus>"#;

  fn test_scanner() -> Scanner {
    scanner(&Service::new(
      "Test",
      SCAN,
      "10.0.0.5",
      8080,
      &[b"rs=/eSCL/".to_vec(), b"TY=Test Scan 1".to_vec()],
    ))
  }

  #[test]
  fn reads_txt_records() {
    let s = test_scanner();
    assert_eq!(s.base_url(), "http://10.0.0.5:8080/eSCL");
    assert_eq!(s.model.as_deref(), Some("Test Scan 1"));
    let v6 = scanner(&Service::new("v6", SCANS, "fd00::1", 443, &[]));
    assert_eq!(v6.base_url(), "https://[fd00::1]:443/eSCL");
    assert!(v6.tls);
  }

  #[test]
  fn reads_capabilities() {
    let caps = parse_capabilities(CAPS).unwrap();
    assert_eq!(caps.version, "2.63");
    assert_eq!(caps.color_modes, [ColorMode::Color, ColorMode::Grayscale]);
    assert_eq!(caps.formats, [Format::Jpeg, Format::Pdf]);
    assert_eq!(caps.resolutions, [300, 600]);
    assert_eq!(caps.sources.len(), 2);
    assert_eq!(caps.sources[1].source, InputSource::Adf);
    assert_eq!(caps.sources[1].max_height, 4200);
    assert_eq!(parse_status(STATUS).as_deref(), Some("Idle"));
    assert!(parse_capabilities("<nope").is_err());
  }

  fn settings(dir: &std::path::Path, source: InputSource) -> ScanSettings {
    ScanSettings {
      resolution: 300,
      color_mode: ColorMode::Grayscale,
      source,
      format: Format::Jpeg,
      destination: dir.to_path_buf(),
    }
  }

  /// as `pages` of the fake scanner: a job that stays busy
  const BUSY: usize = usize::MAX;

  /// a fake eSCL scanner: capabilities, status, one job of `pages` pages
  fn fake_scanner(pages: usize) -> (Arc<dyn HttpClient>, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let seen = log.clone();
    let served = Arc::new(Mutex::new(0));
    let client = FakeHttpClient::create(move |mut req| {
      let url = req.uri().to_string();
      let mut body = String::new();
      block_on(req.body_mut().read_to_string(&mut body)).unwrap();
      seen
        .lock()
        .unwrap()
        .push(format!("{} {url} {body}", req.method()));
      let served = served.clone();
      async move {
        let response = Response::builder();
        let path = url.trim_start_matches("http://10.0.0.5:8080");
        Ok(match path {
          "/eSCL/ScannerCapabilities" => response.status(200).body(CAPS.into())?,
          "/eSCL/ScannerStatus" => response.status(200).body(STATUS.into())?,
          "/eSCL/ScanJobs" => response
            .status(201)
            .header("Location", "/eSCL/ScanJobs/42")
            .body(AsyncBody::empty())?,
          "/eSCL/ScanJobs/42/NextDocument" if pages == BUSY => {
            response.status(503).body(AsyncBody::empty())?
          }
          "/eSCL/ScanJobs/42/NextDocument" => {
            let mut served = served.lock().unwrap();
            if *served < pages {
              *served += 1;
              response.status(200).body(format!("page {served}").into())?
            } else {
              response.status(404).body(AsyncBody::empty())?
            }
          }
          _ => response.status(500).body(AsyncBody::empty())?,
        })
      }
    });
    (client, log)
  }

  #[test]
  fn fetches_capabilities_and_status() {
    let (client, _) = fake_scanner(0);
    let caps = block_on(capabilities(&*client, &test_scanner())).unwrap();
    assert_eq!(caps.state, "Idle");
    assert_eq!(caps.resolutions, [300, 600]);
  }

  #[test]
  fn scans_every_feeder_page() {
    let dir = tempfile::tempdir().unwrap();
    let (client, log) = fake_scanner(2);
    let caps = parse_capabilities(CAPS).unwrap();
    let mut pages = Vec::new();
    let files = block_on(scan(
      &client,
      &test_scanner(),
      &caps,
      &settings(dir.path(), InputSource::Adf),
      "20261019-120000",
      |n| pages.push(n),
    ))
    .unwrap();
    assert_eq!(pages, [1, 2]);
    let names: Vec<_> = files
      .iter()
      .map(|f| f.file_name().unwrap().to_string_lossy().into_owned())
      .collect();
    assert_eq!(
      names,
      ["scan-20261019-120000-1.jpg", "scan-20261019-120000-2.jpg"]
    );
    assert_eq!(std::fs::read_to_string(&files[1]).unwrap(), "page 2");

    let log = log.lock().unwrap();
    let job = &log[0];
    assert!(
      job.starts_with("POST http://10.0.0.5:8080/eSCL/ScanJobs "),
      "{job}"
    );
    for part in [
      "<pwg:Version>2.63</pwg:Version>",
      "<pwg:InputSource>Feeder</pwg:InputSource>",
      "<scan:ColorMode>Grayscale8</scan:ColorMode>",
      "<scan:XResolution>300</scan:XResolution>",
      "<pwg:DocumentFormat>image/jpeg</pwg:DocumentFormat>",
      "<pwg:Height>4200</pwg:Height>",
    ] {
      assert!(job.contains(part), "{part} missing in {job}");
    }
    assert_eq!(log.len(), 4, "job, two pages, the final 404");
  }

  #[test]
  fn flatbed_fetches_one_page() {
    let dir = tempfile::tempdir().unwrap();
    let (client, log) = fake_scanner(5);
    let caps = parse_capabilities(CAPS).unwrap();
    let files = block_on(scan(
      &client,
      &test_scanner(),
      &caps,
      &settings(dir.path(), InputSource::Flatbed),
      "x",
      |_| {},
    ))
    .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(log.lock().unwrap().len(), 2);
  }

  #[test]
  fn scans_over_tls() {
    let dir = tempfile::tempdir().unwrap();
    let port = crate::tls::tests::serve("a", |method, path, body| match (method, path) {
      ("GET", "/eSCL/ScannerCapabilities") => (200, vec![], CAPS.into()),
      ("GET", "/eSCL/ScannerStatus") => (200, vec![], STATUS.into()),
      ("POST", "/eSCL/ScanJobs") if body.contains("<pwg:InputSource>Platen</pwg:InputSource>") => (
        201,
        vec![("Location".into(), "/eSCL/ScanJobs/7".into())],
        vec![],
      ),
      ("GET", "/eSCL/ScanJobs/7/NextDocument") => (200, vec![], b"tls page".to_vec()),
      _ => (404, vec![], vec![]),
    });
    let scanner = scanner(&Service::new("Secure", SCANS, "127.0.0.1", port, &[]));
    let client: Arc<dyn HttpClient> = Arc::new(crate::tls::PinnedClient::new(
      &scanner.name,
      &dir.path().join("pins"),
    ));
    let caps = block_on(capabilities(&*client, &scanner)).unwrap();
    assert_eq!(caps.state, "Idle");
    let files = block_on(scan(
      &client,
      &scanner,
      &caps,
      &settings(dir.path(), InputSource::Flatbed),
      "x",
      |_| {},
    ))
    .unwrap();
    assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), "tls page");
  }

  #[test]
  fn empty_job_fails() {
    let dir = tempfile::tempdir().unwrap();
    let (client, _) = fake_scanner(0);
    let caps = parse_capabilities(CAPS).unwrap();
    let flatbed = settings(dir.path(), InputSource::Flatbed);
    let run = |scanner: Scanner| block_on(scan(&client, &scanner, &caps, &flatbed, "x", |_| {}));
    assert!(run(test_scanner()).is_err());
  }

  #[test]
  fn dropped_scan_deletes_its_job() {
    let dir = tempfile::tempdir().unwrap();
    let (client, log) = fake_scanner(BUSY);
    let caps = parse_capabilities(CAPS).unwrap();
    let flatbed = settings(dir.path(), InputSource::Flatbed);
    let scanner = test_scanner();
    let scan = scan(&client, &scanner, &caps, &flatbed, "x", |_| {});
    // waiting for the busy scanner, then dropped
    assert!(block_on(poll_once(Box::pin(scan))).is_none());
    let deleted = || {
      log
        .lock()
        .unwrap()
        .iter()
        .any(|l| l.starts_with("DELETE http://10.0.0.5:8080/eSCL/ScanJobs/42"))
    };
    let start = std::time::Instant::now();
    while !deleted() && start.elapsed() < Duration::from_secs(5) {
      std::thread::sleep(Duration::from_millis(10));
    }
    assert!(deleted());
  }
}
