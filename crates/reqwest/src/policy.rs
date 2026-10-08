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
  use crate::store::Meta;

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

  #[test]
  fn expires_edges() {
    let cache_control = |value| headers(&[(header::CACHE_CONTROL, value)]);
    assert_eq!(super::expires(&cache_control("max-age=\"60\""), 100), 160);
    assert_eq!(super::expires(&cache_control("MAX-AGE=60"), 100), 160);
    assert_eq!(super::expires(&cache_control("  max-age=60  "), 100), 160);
    assert_eq!(super::expires(&cache_control("max-age=0"), 100), 100);
    // the last max-age wins, an invalid one cancels a valid one
    assert_eq!(
      super::expires(&cache_control("max-age=60, max-age=x"), 100),
      100
    );
    assert_eq!(
      super::expires(&cache_control("max-age=60, max-age=10"), 100),
      110
    );
    assert_eq!(super::expires(&cache_control("max-age=-1"), 100), 100);
    let huge = format!("max-age={}", u64::MAX);
    assert_eq!(super::expires(&cache_control(&huge), 100), u64::MAX);
    // only max-age counts
    assert_eq!(super::expires(&cache_control("s-maxage=60"), 100), 100);
    assert_eq!(
      super::expires(
        &headers(&[(header::EXPIRES, "Wed, 21 Oct 2099 07:28:00 GMT")]),
        100
      ),
      100
    );
    assert_eq!(
      super::expires(&cache_control("NO-CACHE, max-age=60"), 100),
      100
    );

    let mut split = HeaderMap::new();
    split.append(header::CACHE_CONTROL, HeaderValue::from_static("public"));
    split.append(
      header::CACHE_CONTROL,
      HeaderValue::from_static("max-age=30"),
    );
    assert_eq!(super::expires(&split, 100), 130);
    split.append(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    assert_eq!(super::expires(&split, 100), 100);
  }

  #[test]
  fn cacheable_request() {
    let request = |method: Method, header: Option<(header::HeaderName, &str)>| {
      let mut builder = Request::builder().method(method).uri("https://example.org");
      if let Some((name, value)) = header {
        builder = builder.header(name, value);
      }
      builder.body(AsyncBody::empty()).unwrap()
    };
    assert!(super::cacheable_request(&request(Method::GET, None)));
    assert!(super::cacheable_request(&request(
      Method::GET,
      Some((header::CACHE_CONTROL, "no-cache"))
    )));
    assert!(super::cacheable_request(&request(
      Method::GET,
      Some((header::ACCEPT, "*/*"))
    )));
    for method in [Method::POST, Method::HEAD, Method::PUT, Method::DELETE] {
      assert!(!super::cacheable_request(&request(method, None)));
    }
    for name in [
      header::AUTHORIZATION,
      header::RANGE,
      header::IF_NONE_MATCH,
      header::IF_MODIFIED_SINCE,
    ] {
      assert!(!super::cacheable_request(&request(
        Method::GET,
        Some((name, "x"))
      )));
    }
    assert!(!super::cacheable_request(&request(
      Method::GET,
      Some((header::CACHE_CONTROL, "No-Store"))
    )));
  }

  #[test]
  fn storable_metadata() {
    let url = "https://example.org";
    let meta = super::storable(
      url,
      &headers(&[
        (header::CACHE_CONTROL, "max-age=60"),
        (header::ETAG, "\"v1\""),
        (header::LAST_MODIFIED, "yesterday"),
        (header::CONTENT_TYPE, "image/png"),
      ]),
      10,
    )
    .unwrap();
    assert_eq!(meta.url, url);
    assert_eq!(meta.expires, 70);
    assert_eq!(meta.etag.as_deref(), Some("\"v1\""));
    assert_eq!(meta.last_modified.as_deref(), Some("yesterday"));
    assert_eq!(meta.content_type.as_deref(), Some("image/png"));

    // a validator alone is enough, already expired is not
    assert!(super::storable(url, &headers(&[(header::LAST_MODIFIED, "yesterday")]), 0).is_some());
    assert!(super::storable(url, &headers(&[(header::CACHE_CONTROL, "max-age=0")]), 5).is_none());
    assert!(super::storable(url, &headers(&[(header::CACHE_CONTROL, "no-cache")]), 5).is_none());
    // non-ASCII header values are ignored rather than stored mangled
    let mut odd = headers(&[(header::CACHE_CONTROL, "max-age=60")]);
    odd.insert(header::ETAG, HeaderValue::from_bytes(b"\"\xff\"").unwrap());
    assert_eq!(super::storable(url, &odd, 0).unwrap().etag, None);
  }

  #[test]
  fn refresh() {
    let mut meta = Meta {
      url: "u".into(),
      expires: 0,
      etag: Some("\"v1\"".into()),
      last_modified: Some("old".into()),
      content_type: Some("image/png".into()),
    };
    super::refresh(&mut meta, &HeaderMap::new(), 50);
    assert_eq!(meta.expires, 50);
    assert_eq!(meta.etag.as_deref(), Some("\"v1\""));
    assert_eq!(meta.last_modified.as_deref(), Some("old"));

    super::refresh(
      &mut meta,
      &headers(&[
        (header::CACHE_CONTROL, "max-age=10"),
        (header::ETAG, "\"v2\""),
        (header::LAST_MODIFIED, "new"),
        (header::CONTENT_TYPE, "text/plain"),
      ]),
      50,
    );
    assert_eq!(meta.expires, 60);
    assert_eq!(meta.etag.as_deref(), Some("\"v2\""));
    assert_eq!(meta.last_modified.as_deref(), Some("new"));
    // a 304 does not change the stored body's type
    assert_eq!(meta.content_type.as_deref(), Some("image/png"));
  }

  #[test]
  fn cacheable_request_with_other_conditional_headers() {
    let request = |(name, value)| {
      let mut builder = Request::builder().method(Method::GET);
      builder = builder.header(name, value);
      builder.body(AsyncBody::empty()).unwrap()
    };
    assert!(super::cacheable_request(&request((header::IF_MATCH, "\"xyz\""))));
    assert!(super::cacheable_request(&request((header::IF_UNMODIFIED_SINCE, "yesterday"))));
    assert!(super::cacheable_request(&request((header::IF_RANGE, "\"xyz\""))));
  }

  #[test]
  fn expires_whitespace_edges() {
    let cache_control = |value| headers(&[(header::CACHE_CONTROL, value)]);
    assert_eq!(super::expires(&cache_control("max-age = 60"), 100), 100);
    assert_eq!(super::expires(&cache_control("max-age=\" 60 \""), 100), 100);
  }

  #[test]
  fn refresh_without_cache_control_expires_at_now() {
    let mut meta = Meta {
      url: "u".into(),
      expires: 1000,
      etag: Some("\"v1\"".into()),
      last_modified: None,
      content_type: None,
    };
    super::refresh(&mut meta, &HeaderMap::new(), 50);
    assert_eq!(meta.expires, 50);
  }
}
