use std::io;

use reqwest_client::ReqwestClient;

pub use crate::client::CachedHttpClient;

mod cleanup;
mod client;
mod policy;
mod store;

pub fn client() -> io::Result<CachedHttpClient<ReqwestClient>> {
  let client = ReqwestClient::user_agent(concat!("corona/", env!("CARGO_PKG_VERSION")))
    .map_err(io::Error::other)?;
  let dir = dirs::cache_dir()
    .unwrap_or_else(std::env::temp_dir)
    .join("corona")
    .join("http");
  CachedHttpClient::new(client, dir)
}

#[cfg(test)]
fn temp_dir(name: &str) -> std::path::PathBuf {
  let dir = std::env::temp_dir().join(format!("corona-http-cache-{name}-{}", std::process::id()));
  std::fs::remove_dir_all(&dir).ok();
  std::fs::create_dir_all(&dir).unwrap();
  dir
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_client_constructs() {
    let client = client();
    assert!(client.is_ok());
  }
}

