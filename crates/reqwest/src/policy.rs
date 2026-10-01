use http_client::{
  AsyncBody, Method, Request,
  http::{HeaderMap, header},
};

use crate::store::Meta;

pub(crate) fn cacheable_request(req: &Request<AsyncBody>) -> bool {
  let headers = req.headers();
  // conditional, partial and authorized requests are the caller's own business
  req.method() == Method::GET
    && !headers.contains_key(header::AUTHORIZATION)
    && !headers.contains_key(header::RANGE)
    && !headers.contains_key(header::IF_NONE_MATCH)
    && !headers.contains_key(header::IF_MODIFIED_SINCE)
    && !cache_control(headers).any(|directive| directive == "no-store")
}

pub(crate) fn expires(headers: &HeaderMap, now: u64) -> u64 {
  let mut max_age = None;
  for directive in cache_control(headers) {
    if directive == "no-cache" {
      return now;
    }
    if let Some(seconds) = directive.strip_prefix("max-age=") {
      max_age = seconds.trim_matches('"').parse::<u64>().ok();
    }
  }
  max_age.map_or(now, |seconds| now.saturating_add(seconds))
}

pub(crate) fn storable(url: &str, headers: &HeaderMap, now: u64) -> Option<Meta> {
  if cache_control(headers).any(|directive| directive == "no-store") {
    return None;
  }
  let meta = Meta {
    url: url.to_string(),
    expires: expires(headers, now),
    etag: header_string(headers, header::ETAG),
    last_modified: header_string(headers, header::LAST_MODIFIED),
    content_type: header_string(headers, header::CONTENT_TYPE),
  };
  (meta.expires > now || meta.revalidatable()).then_some(meta)
}

pub(crate) fn refresh(meta: &mut Meta, headers: &HeaderMap, now: u64) {
  meta.expires = expires(headers, now);
  if let Some(etag) = header_string(headers, header::ETAG) {
    meta.etag = Some(etag);
  }
  if let Some(date) = header_string(headers, header::LAST_MODIFIED) {
    meta.last_modified = Some(date);
  }
}

fn cache_control(headers: &HeaderMap) -> impl Iterator<Item = String> + '_ {
  headers
    .get_all(header::CACHE_CONTROL)
    .iter()
    .filter_map(|value| value.to_str().ok())
    .flat_map(|value| value.split(','))
    .map(|directive| directive.trim().to_ascii_lowercase())
}

fn header_string(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
  Some(headers.get(name)?.to_str().ok()?.to_string())
}

#[cfg(test)]
mod tests {
  use http_client::http::HeaderValue;

  use super::*;

  fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
    pairs
      .iter()
      .map(|(name, value)| (name.clone(), HeaderValue::from_str(value).unwrap()))
      .collect()
  }

  #[test]
  fn expires() {
    let cache_control = |value| headers(&[(header::CACHE_CONTROL, value)]);
    assert_eq!(
      super::expires(&cache_control("public, max-age=60"), 100),
      160
    );
    assert_eq!(
      super::expires(&cache_control("max-age=60, no-cache"), 100),
      100
    );
    assert_eq!(super::expires(&HeaderMap::new(), 100), 100);
  }

  #[test]
  fn storable() {
    let url = "https://example.org";
    assert!(super::storable(url, &headers(&[(header::CACHE_CONTROL, "max-age=60")]), 0).is_some());
    assert!(super::storable(url, &headers(&[(header::ETAG, "\"v1\"")]), 0).is_some());
    // nothing to reuse it with
    assert!(super::storable(url, &HeaderMap::new(), 0).is_none());
    let no_store = headers(&[
      (header::CACHE_CONTROL, "no-store"),
      (header::ETAG, "\"v1\""),
    ]);
    assert!(super::storable(url, &no_store, 0).is_none());
  }
}
