//! What a plugin may do. Empty by default: no file, process, network, storage or clipboard
//! access until its manifest grants it.

use std::{
  ffi::OsString,
  fmt,
  path::{Component, Path, PathBuf},
  sync::{Arc, Mutex},
};

use cap_std::{ambient_authority, fs::Dir};

/// Which commands a plugin may run
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ExecuteGrant {
  #[default]
  Denied,
  /// Only these command names
  Allowed(Vec<String>),
  /// Any command
  Unrestricted,
}

/// HTTP to one host, scoped by scheme, effective port, method and path. Separate from
/// network hosts: it grants no TCP or WebSocket access to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpRequestGrant {
  scheme: String,
  host: String,
  port: Option<u16>,
  methods: Vec<String>,
  paths: Vec<String>,
  path_prefixes: Vec<String>,
}

impl HttpRequestGrant {
  /// An HTTPS grant for `host`
  pub fn new(
    host: impl Into<String>,
    methods: impl IntoIterator<Item = impl Into<String>>,
    paths: impl IntoIterator<Item = impl Into<String>>,
    path_prefixes: impl IntoIterator<Item = impl Into<String>>,
  ) -> Self {
    Self {
      scheme: "https".into(),
      host: host.into().to_ascii_lowercase(),
      port: None,
      methods: methods
        .into_iter()
        .map(|m| m.into().to_ascii_uppercase())
        .collect(),
      paths: paths.into_iter().map(Into::into).collect(),
      path_prefixes: path_prefixes.into_iter().map(Into::into).collect(),
    }
  }

  pub fn scheme(mut self, scheme: impl Into<String>) -> Self {
    self.scheme = scheme.into().to_ascii_lowercase();
    self
  }

  pub fn port(mut self, port: u16) -> Self {
    self.port = Some(port);
    self
  }

  fn allows(&self, scheme: &str, host: &str, port: Option<u16>, method: &str, path: &str) -> bool {
    // an encoded separator can name another path on a server that decodes it
    let lower = path.to_ascii_lowercase();
    let ambiguous = lower.contains("%2f") || lower.contains("%5c") || path.contains('\\');
    let prefix = |p: &String| {
      !ambiguous
        && path
          .strip_prefix(p.as_str())
          .is_some_and(|rest| rest.is_empty() || p.ends_with('/') || rest.starts_with('/'))
    };
    self.scheme.eq_ignore_ascii_case(scheme)
      && self.host.eq_ignore_ascii_case(host)
      && effective_port(&self.scheme, self.port) == effective_port(scheme, port)
      && self.methods.iter().any(|m| m.eq_ignore_ascii_case(method))
      && (self.paths.iter().any(|p| p == path) || self.path_prefixes.iter().any(prefix))
  }
}

pub(crate) fn effective_port(scheme: &str, port: Option<u16>) -> Option<u16> {
  port.or(match scheme {
    "http" | "ws" => Some(80),
    "https" | "wss" => Some(443),
    _ => None,
  })
}

/// A plugin's grant. Clones share the files the user picked ([`grant_read`](Self::grant_read)).
#[derive(Clone, Default)]
pub struct Capabilities {
  read_roots: Vec<PathBuf>,
  write_roots: Vec<PathBuf>,
  execute: ExecuteGrant,
  process_env: Vec<String>,
  network_hosts: Vec<String>,
  network_lan_hosts: Vec<String>,
  network_unix: Vec<PathBuf>,
  asset_root: Option<PathBuf>,
  http_requests: Vec<HttpRequestGrant>,
  storage: bool,
  clipboard_read: bool,
  clipboard_write: bool,
  exit: bool,
  print: bool,
  print_manage: bool,
  /// Resolved picks, and whether each is a directory
  picked: Arc<Mutex<Vec<(PathBuf, bool)>>>,
}

/// Leaves out the picks: what the manifest granted
impl fmt::Debug for Capabilities {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Capabilities")
      .field("read_roots", &self.read_roots)
      .field("write_roots", &self.write_roots)
      .field("execute", &self.execute)
      .field("process_env", &self.process_env)
      .field("network_hosts", &self.network_hosts)
      .field("network_lan_hosts", &self.network_lan_hosts)
      .field("network_unix", &self.network_unix)
      .field("asset_root", &self.asset_root)
      .field("http_requests", &self.http_requests)
      .field("storage", &self.storage)
      .field("clipboard_read", &self.clipboard_read)
      .field("clipboard_write", &self.clipboard_write)
      .field("exit", &self.exit)
      .field("print", &self.print)
      .field("print_manage", &self.print_manage)
      .finish()
  }
}

fn lower(hosts: impl IntoIterator<Item = String>) -> Vec<String> {
  hosts.into_iter().map(|h| h.to_ascii_lowercase()).collect()
}

impl Capabilities {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn read_roots(mut self, roots: impl IntoIterator<Item = PathBuf>) -> Self {
    self.read_roots = roots.into_iter().collect();
    self
  }

  pub fn write_roots(mut self, roots: impl IntoIterator<Item = PathBuf>) -> Self {
    self.write_roots = roots.into_iter().collect();
    self
  }

  pub fn execute(mut self, grant: ExecuteGrant) -> Self {
    self.execute = grant;
    self
  }

  /// Host variables a child inherits, and the only names a plugin may set for one.
  pub fn process_env(mut self, names: impl IntoIterator<Item = String>) -> Self {
    self.process_env = names.into_iter().collect();
    self
  }

  pub fn network_hosts(mut self, hosts: impl IntoIterator<Item = String>) -> Self {
    self.network_hosts = lower(hosts);
    self
  }

  /// Hosts that may also resolve to private addresses; loopback and link-local stay closed.
  pub fn network_lan_hosts(mut self, hosts: impl IntoIterator<Item = String>) -> Self {
    self.network_lan_hosts = lower(hosts);
    self
  }

  /// Unix sockets, as exact canonical paths
  pub fn network_unix(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
    self.network_unix = paths.into_iter().collect();
    self
  }

  /// The directory relative image paths resolve against
  pub fn asset_root(mut self, path: PathBuf) -> Self {
    self.asset_root = Some(path);
    self
  }

  pub fn http_requests(mut self, grants: impl IntoIterator<Item = HttpRequestGrant>) -> Self {
    self.http_requests = grants.into_iter().collect();
    self
  }

  pub fn storage(mut self, allowed: bool) -> Self {
    self.storage = allowed;
    self
  }

  pub fn clipboard_read(mut self, allowed: bool) -> Self {
    self.clipboard_read = allowed;
    self
  }

  pub fn clipboard_write(mut self, allowed: bool) -> Self {
    self.clipboard_write = allowed;
    self
  }

  pub fn exit(mut self, allowed: bool) -> Self {
    self.exit = allowed;
    self
  }

  pub fn print(mut self, allowed: bool) -> Self {
    self.print = allowed;
    self
  }

  pub fn print_manage(mut self, allowed: bool) -> Self {
    self.print_manage = allowed;
    self
  }

  /// Makes a path the user picked readable: a file alone, a directory as a read root.
  /// ponytail: grows with every pick until the plugin restarts
  pub fn grant_read(&self, path: PathBuf) {
    // resolved now, so a link swapped in later never moves the grant
    let Ok(path) = path.canonicalize() else {
      return;
    };
    let is_dir = path.is_dir();
    self.picked.lock().unwrap().push((path, is_dir));
  }

  /// Whether the manifest grants any directory to read
  pub fn has_read_access(&self) -> bool {
    !self.read_roots.is_empty()
  }

  /// Whether the manifest grants any directory to write
  pub fn has_write_access(&self) -> bool {
    !self.write_roots.is_empty()
  }

  pub fn may_pass_env(&self, name: &str) -> bool {
    self.process_env.iter().any(|n| n == name)
  }

  pub fn process_env_names(&self) -> &[String] {
    &self.process_env
  }

  pub fn execute_grant(&self) -> &ExecuteGrant {
    &self.execute
  }

  pub fn may_run(&self, command: &str) -> bool {
    match &self.execute {
      ExecuteGrant::Denied => false,
      ExecuteGrant::Unrestricted => true,
      ExecuteGrant::Allowed(names) => names.iter().any(|n| n == command),
    }
  }

  pub fn may_reach(&self, host: &str) -> bool {
    (self.network_hosts.iter())
      .chain(&self.network_lan_hosts)
      .any(|h| h.eq_ignore_ascii_case(host))
  }

  pub fn may_reach_lan(&self, host: &str) -> bool {
    self
      .network_lan_hosts
      .iter()
      .any(|h| h.eq_ignore_ascii_case(host))
  }

  /// The hosts that may resolve to private addresses
  pub fn lan_hosts(&self) -> &[String] {
    &self.network_lan_hosts
  }

  /// Whether an HTTP request is granted, by a host grant or a scoped HTTP grant.
  pub fn may_request(
    &self,
    scheme: &str,
    host: &str,
    port: Option<u16>,
    method: &str,
    path: &str,
  ) -> bool {
    self.may_reach(host)
      || (self.http_requests.iter()).any(|g| g.allows(scheme, host, port, method, path))
  }

  /// The canonical socket `path` names when that is granted; connect to it. A link planted
  /// under a granted name resolves elsewhere and no longer matches.
  pub fn unix_socket(&self, path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok().filter(|_| path.is_absolute())?;
    self.network_unix.contains(&canonical).then_some(canonical)
  }

  pub fn asset_root_path(&self) -> Option<&Path> {
    self.asset_root.as_deref()
  }

  pub fn has_storage(&self) -> bool {
    self.storage
  }

  pub fn is_clipboard_readable(&self) -> bool {
    self.clipboard_read
  }

  pub fn is_clipboard_writable(&self) -> bool {
    self.clipboard_write
  }

  pub fn may_exit(&self) -> bool {
    self.exit
  }

  /// Whether `corona/printing` may print files
  pub fn may_print(&self) -> bool {
    self.print
  }

  /// Whether `corona/printing` may scan, add or pause printers and cancel jobs
  pub fn may_manage_printing(&self) -> bool {
    self.print_manage
  }

  /// Whether `path` lies under a read grant, for a host handing it out of the sandbox
  pub fn may_read(&self, path: &Path) -> bool {
    self.open(path, Access::Read).is_ok()
  }

  /// The granted directory `path` belongs to, opened, and the path within it. Every
  /// operation goes through the handle, which refuses to resolve outside itself, so a
  /// symlink out of the root, also one planted after this check, is not followed.
  pub fn open(&self, path: &Path, access: Access) -> Result<Grant, CapabilityError> {
    let (roots, picked) = match access {
      Access::Read => (&self.read_roots, self.picked.lock().unwrap().clone()),
      Access::Write => (&self.write_roots, Vec::new()),
    };
    if roots.is_empty() && picked.is_empty() {
      return Err(CapabilityError::NotGranted(access));
    }
    let (dirs, files): (Vec<_>, Vec<_>) = picked.into_iter().partition(|(_, dir)| *dir);

    // a picked file grants itself alone: its directory, narrowed to its name
    if path.is_absolute()
      && let Some(target) = resolved(path)
      && files.iter().any(|(file, _)| *file == target)
      && let (Some(parent), Some(name)) = (target.parent(), target.file_name())
      && let Ok(dir) = Dir::open_ambient_dir(parent, ambient_authority())
    {
      return Ok(Grant {
        dir,
        relative: name.into(),
      });
    }

    for root in roots.iter().chain(dirs.iter().map(|(dir, _)| dir)) {
      let Some(root) = resolved(root) else {
        continue;
      };
      let full = match path.is_absolute() {
        true => resolved(path).unwrap_or_else(|| normalize(path)),
        // lexical first, so `../..` is refused by the grant, not an errno
        false => normalize(&root.join(path)),
      };
      let Ok(relative) = full.strip_prefix(&root) else {
        continue;
      };
      // a write grant names the plugin's directory, which need not exist yet
      if access == Access::Write && !root.exists() {
        let _ = std::fs::create_dir_all(&root);
      }
      let Ok(dir) = Dir::open_ambient_dir(&root, ambient_authority()) else {
        continue;
      };
      let relative = match relative.as_os_str().is_empty() {
        true => PathBuf::from("."),
        false => relative.to_path_buf(),
      };
      return Ok(Grant { dir, relative });
    }
    Err(CapabilityError::OutsideRoots {
      path: path.to_path_buf(),
      access,
    })
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
  Read,
  Write,
}

impl fmt::Display for Access {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Access::Read => "read",
      Access::Write => "write",
    })
  }
}

#[derive(Debug, PartialEq)]
pub enum CapabilityError {
  NotGranted(Access),
  OutsideRoots { path: PathBuf, access: Access },
  ExecuteDenied(String),
  EnvDenied(String),
  StorageDenied,
}

impl fmt::Display for CapabilityError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::NotGranted(access) => write!(
        f,
        "filesystem {access} is not granted; declare capabilities.fs.{access} in the manifest"
      ),
      Self::OutsideRoots { path, access } => write!(
        f,
        "`{}` is outside every granted {access} root",
        path.display()
      ),
      Self::ExecuteDenied(command) => write!(
        f,
        "running `{command}` is not granted; add it to capabilities.fs.execute in the manifest"
      ),
      Self::EnvDenied(name) => write!(
        f,
        "environment variable `{name}` is not granted; add it to capabilities.process.env in the manifest"
      ),
      Self::StorageDenied => {
        f.write_str("storage is not granted; set capabilities.storage to true")
      }
    }
  }
}

impl std::error::Error for CapabilityError {}

/// A granted directory and the path within it, never absolute, never `..`
#[derive(Debug)]
pub struct Grant {
  pub dir: Dir,
  pub relative: PathBuf,
}

/// Canonicalizes the deepest existing ancestor and keeps the missing rest verbatim: a
/// granted directory need not exist yet. `None` for a dangling link at the boundary.
fn resolved(path: &Path) -> Option<PathBuf> {
  let mut tail: Vec<OsString> = Vec::new();
  let mut current = path.to_path_buf();
  let mut out = loop {
    if let Ok(real) = current.canonicalize() {
      break real;
    }
    tail.push(current.file_name()?.to_os_string());
    if !current.pop() {
      return None;
    }
  };
  out.extend(tail.iter().rev());
  Some(out)
}

fn normalize(path: &Path) -> PathBuf {
  let mut out = PathBuf::new();
  for component in path.components() {
    match component {
      Component::ParentDir => {
        out.pop();
      }
      Component::CurDir => {}
      other => out.push(other),
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn http_grants_match_scheme_port_method_and_path() {
    let grant = HttpRequestGrant::new("API.example.com", ["get"], ["/v1/x"], ["/v1/items"]);
    // (scheme, host, port, method, path)
    let cases = [
      ("https", "api.example.com", None, "GET", "/v1/x", true),
      ("https", "api.example.com", Some(443), "get", "/v1/x", true),
      (
        "https",
        "api.example.com",
        Some(8443),
        "GET",
        "/v1/x",
        false,
      ),
      ("http", "api.example.com", None, "GET", "/v1/x", false),
      ("https", "api.example.com", None, "POST", "/v1/x", false),
      ("https", "other.example.com", None, "GET", "/v1/x", false),
      ("https", "api.example.com", None, "GET", "/v1/x/", false),
      ("https", "api.example.com", None, "GET", "/v1/items", true),
      ("https", "api.example.com", None, "GET", "/v1/items/3", true),
      ("https", "api.example.com", None, "GET", "/v1/itemsx", false),
      (
        "https",
        "api.example.com",
        None,
        "GET",
        "/v1/items/a%2F..%2Fb",
        false,
      ),
      (
        "https",
        "api.example.com",
        None,
        "GET",
        "/v1/items/a%5c..",
        false,
      ),
      (
        "https",
        "api.example.com",
        None,
        "GET",
        "/v1/items/a\\b",
        false,
      ),
    ];
    for (scheme, host, port, method, path, want) in cases {
      assert_eq!(
        grant.allows(scheme, host, port, method, path),
        want,
        "{scheme} {host} {port:?} {method} {path}"
      );
    }
    let slash = HttpRequestGrant::new("h", ["GET"], [] as [&str; 0], ["/a/"]);
    assert!(slash.allows("https", "h", None, "GET", "/a/b"));
    assert!(!slash.allows("https", "h", None, "GET", "/a"));
    let plain = HttpRequestGrant::new("h", ["GET"], ["/"], [] as [&str; 0])
      .scheme("HTTP")
      .port(8080);
    assert!(plain.allows("http", "h", Some(8080), "GET", "/"));
    assert!(!plain.allows("http", "h", None, "GET", "/"));
  }

  #[test]
  fn unix_sockets_are_matched_canonically() {
    let dir = std::env::temp_dir().join(format!("corona-caps-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let real = dir.canonicalize().unwrap().join("real.sock");
    std::fs::write(&real, "").unwrap();
    let link = dir.join("link.sock");
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink("/etc/hostname", &link).unwrap();
    let caps = Capabilities::new().network_unix([real.clone()]);
    assert_eq!(caps.unix_socket(&real), Some(real.clone()));
    assert_eq!(caps.unix_socket(&link), None);
    assert_eq!(caps.unix_socket(Path::new("relative.sock")), None);
    assert_eq!(caps.unix_socket(&dir.join("missing.sock")), None);
    let _ = std::fs::remove_dir_all(&dir);
  }
}
