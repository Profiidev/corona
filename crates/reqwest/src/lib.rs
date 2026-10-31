use std::io;

use anyhow::{Context, Result, bail};
use futures::AsyncReadExt;
use http_client::{AsyncBody, HttpClient};
use reqwest_client::ReqwestClient;
use serde::de::DeserializeOwned;

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

/// GETs `url` and parses its JSON, a failed status errors with its body
pub async fn get_json<T: DeserializeOwned>(client: &dyn HttpClient, url: &str) -> Result<T> {
  let mut response = client.get(url, AsyncBody::empty(), true).await?;
  let mut body = String::new();
  response.body_mut().read_to_string(&mut body).await?;
  if !response.status().is_success() {
    bail!("{} answered {}: {}", url, response.status(), body.trim());
  }
  serde_json::from_str(&body).with_context(|| format!("parsing {url}"))
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
