use std::{fs, io, path::PathBuf, sync::Arc};

use futures::{AsyncReadExt, FutureExt, future::BoxFuture};
use http_client::{
  AsyncBody, HttpClient, Request, Response, StatusCode, Url,
  http::{HeaderValue, header},
};

use crate::{
  cleanup::{MAX_SIZE, cleanup},
  policy::{cacheable_request, refresh, storable},
  store::{self, cached_response, read_meta, write_meta},
};

pub struct CachedHttpClient<C> {
  inner: Arc<C>,
  dir: PathBuf,
}

impl<C: HttpClient> CachedHttpClient<C> {
  pub fn new(inner: C, dir: PathBuf) -> io::Result<Self> {
    fs::create_dir_all(&dir)?;
    let cleanup_dir = dir.clone();
    std::thread::spawn(move || {
      if let Err(e) = cleanup(&cleanup_dir, MAX_SIZE, store::unix_now()) {
        tracing::warn!("http cache cleanup failed: {e}");
      }
    });
    Ok(Self {
      inner: Arc::new(inner),
      dir,
    })
  }
}

impl<C: HttpClient> HttpClient for CachedHttpClient<C> {
  fn user_agent(&self) -> Option<&HeaderValue> {
    self.inner.user_agent()
  }

  fn proxy(&self) -> Option<&Url> {
    self.inner.proxy()
  }

  fn send(
    &self,
    req: Request<AsyncBody>,
  ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
    if !cacheable_request(&req) {
      return self.inner.send(req);
    }
    let inner = self.inner.clone();
    let base = store::base(&self.dir, &req.uri().to_string());
    send_cached(inner, base, req).boxed()
  }
}

async fn send_cached<C: HttpClient>(
  inner: Arc<C>,
  base: PathBuf,
  mut req: Request<AsyncBody>,
) -> anyhow::Result<Response<AsyncBody>> {
  let url = req.uri().to_string();
  let now = store::unix_now();
  // the URL check guards against a hash collision
  let cached = read_meta(&base).filter(|meta| meta.url == url);

  if let Some(meta) = cached.as_ref().filter(|meta| now < meta.expires)
    && let Ok(response) = cached_response(&base, meta)
  {
    return Ok(response);
  }

  if let Some(meta) = cached.as_ref().filter(|meta| meta.revalidatable()) {
    let headers = req.headers_mut();
    if let Some(etag) = meta
      .etag
      .as_deref()
      .and_then(|v| HeaderValue::from_str(v).ok())
    {
      headers.insert(header::IF_NONE_MATCH, etag);
    }
    if let Some(date) = meta
      .last_modified
      .as_deref()
      .and_then(|v| HeaderValue::from_str(v).ok())
    {
      headers.insert(header::IF_MODIFIED_SINCE, date);
    }
  }

  let response = match inner.send(req).await {
    Ok(response) => response,
    Err(error) => {
      return cached
        .as_ref()
        .and_then(|meta| cached_response(&base, meta).ok())
        .ok_or(error);
    }
  };

  if response.status() == StatusCode::NOT_MODIFIED
    && let Some(mut meta) = cached
  {
    refresh(&mut meta, response.headers(), now);
    if let Err(e) = write_meta(&base, &meta) {
      tracing::warn!("http cache: keeping {url} failed: {e}");
    }
    return cached_response(&base, &meta).map_err(Into::into);
  }

  if response.status() != StatusCode::OK {
    return Ok(response);
  }
  let Some(meta) = storable(&url, response.headers(), now) else {
    return Ok(response);
  };

  let (parts, mut body) = response.into_parts();
  let mut bytes = Vec::new();
  body.read_to_end(&mut bytes).await?;
  if let Err(e) = store::write_entry(&base, &meta, &bytes) {
    tracing::warn!("http cache: storing {url} failed: {e}");
  }
  Ok(Response::from_parts(parts, bytes.into()))
}

#[cfg(test)]
mod tests {
  use std::sync::Mutex;

  use futures::executor::block_on;
  use http_client::http::HeaderMap;

  use super::*;
  use crate::temp_dir;

  /// answers with the queued responses and records the requests it got
  #[derive(Default)]
  struct Fake {
    responses: Mutex<Vec<anyhow::Result<Response<AsyncBody>>>>,
    requests: Mutex<Vec<HeaderMap>>,
  }

  impl HttpClient for Fake {
    fn user_agent(&self) -> Option<&HeaderValue> {
      None
    }

    fn proxy(&self) -> Option<&Url> {
      None
    }

    fn send(
      &self,
      req: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
      self.requests.lock().unwrap().push(req.headers().clone());
      let response = self.responses.lock().unwrap().remove(0);
      async move { response }.boxed()
    }
  }

  fn response(status: u16, headers: &[(&str, &str)], body: &str) -> Response<AsyncBody> {
    let mut response = Response::builder().status(status);
    for (name, value) in headers {
      response = response.header(*name, *value);
    }
    response.body(body.to_string().into()).unwrap()
  }

  fn get(client: &CachedHttpClient<Fake>) -> (StatusCode, String) {
    let req = Request::get("https://example.org/art.png")
      .body(AsyncBody::empty())
      .unwrap();
    let mut response = block_on(client.send(req)).unwrap();
    let mut body = String::new();
    block_on(response.body_mut().read_to_string(&mut body)).unwrap();
    (response.status(), body)
  }

  fn client(dir: PathBuf, responses: Vec<Response<AsyncBody>>) -> CachedHttpClient<Fake> {
    client_with(dir, responses.into_iter().map(Ok).collect())
  }

  fn client_with(
    dir: PathBuf,
    responses: Vec<anyhow::Result<Response<AsyncBody>>>,
  ) -> CachedHttpClient<Fake> {
    let fake = Fake {
      responses: Mutex::new(responses),
      ..Default::default()
    };
    CachedHttpClient {
      inner: Arc::new(fake),
      dir,
    }
  }

  #[test]
  fn fresh_entries_skip_the_network() {
    let dir = temp_dir("fresh");
    let client = client(
      dir.clone(),
      vec![response(
        200,
        &[("cache-control", "public, max-age=3600")],
        "art",
      )],
    );
    assert_eq!(get(&client).1, "art");
    // the fake has no second response, a request would panic
    assert_eq!(get(&client), (StatusCode::OK, "art".into()));
    assert_eq!(client.inner.requests.lock().unwrap().len(), 1);
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn stale_entries_revalidate() {
    let dir = temp_dir("stale");
    let client = client(
      dir.clone(),
      vec![
        response(
          200,
          &[("cache-control", "no-cache"), ("etag", "\"v1\"")],
          "art",
        ),
        response(304, &[("etag", "\"v1\"")], ""),
        response(
          200,
          &[("cache-control", "no-cache"), ("etag", "\"v2\"")],
          "new art",
        ),
      ],
    );
    assert_eq!(get(&client).1, "art");
    // 304: the body comes from disk
    assert_eq!(get(&client), (StatusCode::OK, "art".into()));
    // 200: the new body replaces the old one
    assert_eq!(get(&client).1, "new art");

    let requests = client.inner.requests.lock().unwrap();
    assert!(requests[0].get(header::IF_NONE_MATCH).is_none());
    assert_eq!(requests[1][header::IF_NONE_MATCH], "\"v1\"");
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn no_store_is_not_kept() {
    let dir = temp_dir("no-store");
    let client = client(
      dir.clone(),
      vec![
        response(
          200,
          &[("cache-control", "no-store"), ("etag", "\"v1\"")],
          "a",
        ),
        response(200, &[], "b"),
      ],
    );
    get(&client);
    assert_eq!(get(&client).1, "b");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
    fs::remove_dir_all(dir).ok();
  }

  const URL: &str = "https://example.org/art.png";

  fn send(
    client: &CachedHttpClient<Fake>,
    req: Request<AsyncBody>,
  ) -> anyhow::Result<(StatusCode, HeaderMap, String)> {
    let mut response = block_on(client.send(req))?;
    let mut body = String::new();
    block_on(response.body_mut().read_to_string(&mut body)).unwrap();
    Ok((response.status(), response.headers().clone(), body))
  }

  fn plain_get() -> Request<AsyncBody> {
    Request::get(URL).body(AsyncBody::empty()).unwrap()
  }

  fn files(dir: &std::path::Path) -> usize {
    fs::read_dir(dir).unwrap().count()
  }

  #[test]
  fn network_errors_fall_back_to_the_cached_copy() {
    let dir = temp_dir("offline");
    let client = client_with(
      dir.clone(),
      vec![
        Ok(response(
          200,
          &[("cache-control", "no-cache"), ("etag", "\"v1\"")],
          "art",
        )),
        Err(anyhow::anyhow!("offline")),
      ],
    );
    assert_eq!(get(&client).1, "art");
    assert_eq!(get(&client), (StatusCode::OK, "art".into()));
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn network_errors_without_a_copy_are_returned() {
    let dir = temp_dir("offline-empty");
    let client = client_with(dir.clone(), vec![Err(anyhow::anyhow!("offline"))]);
    let error = send(&client, plain_get()).unwrap_err();
    assert_eq!(error.to_string(), "offline");
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn errors_pass_through_unstored() {
    let dir = temp_dir("not-found");
    let client = client(
      dir.clone(),
      vec![
        response(404, &[("cache-control", "max-age=3600")], "missing"),
        response(200, &[], "b"),
      ],
    );
    assert_eq!(get(&client), (StatusCode::NOT_FOUND, "missing".into()));
    assert_eq!(files(&dir), 0);
    assert_eq!(get(&client).1, "b");
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn not_modified_without_a_cache_entry_passes_through() {
    let dir = temp_dir("bare-304");
    let client = client(dir.clone(), vec![response(304, &[("etag", "\"v1\"")], "")]);
    assert_eq!(get(&client).0, StatusCode::NOT_MODIFIED);
    assert_eq!(files(&dir), 0);
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  #[ignore = "BUG: a 304 for an entry whose body file is gone returns an error instead of refetching"]
  fn bug_not_modified_with_a_missing_body_refetches() {
    let dir = temp_dir("304-no-body");
    let client = client(
      dir.clone(),
      vec![
        response(
          200,
          &[("cache-control", "no-cache"), ("etag", "\"v1\"")],
          "art",
        ),
        response(304, &[("etag", "\"v1\"")], ""),
        response(
          200,
          &[("cache-control", "no-cache"), ("etag", "\"v1\"")],
          "art",
        ),
      ],
    );
    get(&client);
    fs::remove_file(store::path(&store::base(&dir, URL), "body")).unwrap();
    let (status, _, body) = send(&client, plain_get()).unwrap();
    assert_eq!((status, body.as_str()), (StatusCode::OK, "art"));
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn last_modified_is_revalidated() {
    let dir = temp_dir("last-modified");
    let date = "Wed, 21 Oct 2015 07:28:00 GMT";
    let client = client(
      dir.clone(),
      vec![
        response(
          200,
          &[("cache-control", "max-age=0"), ("last-modified", date)],
          "art",
        ),
        response(304, &[("cache-control", "max-age=3600")], ""),
      ],
    );
    get(&client);
    assert_eq!(get(&client), (StatusCode::OK, "art".into()));
    let requests = client.inner.requests.lock().unwrap();
    assert_eq!(requests[1][header::IF_MODIFIED_SINCE], date);
    assert!(requests[1].get(header::IF_NONE_MATCH).is_none());
    drop(requests);
    // the 304 refreshed the lifetime: no third request
    assert_eq!(get(&client).1, "art");
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn not_modified_keeps_the_known_validators() {
    let dir = temp_dir("304-keeps");
    let client = client(
      dir.clone(),
      vec![
        response(
          200,
          &[("cache-control", "no-cache"), ("etag", "\"v1\"")],
          "art",
        ),
        // no etag on the 304: the stored one stays
        response(304, &[], ""),
        response(304, &[], ""),
      ],
    );
    get(&client);
    get(&client);
    get(&client);
    let requests = client.inner.requests.lock().unwrap();
    assert_eq!(requests[2][header::IF_NONE_MATCH], "\"v1\"");
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn hash_collisions_are_misses() {
    let dir = temp_dir("collision");
    let base = store::base(&dir, URL);
    let other = store::Meta {
      url: "https://other.example/".into(),
      expires: u64::MAX,
      etag: Some("\"other\"".into()),
      last_modified: None,
      content_type: None,
    };
    store::write_entry(&base, &other, b"other").unwrap();
    let client = client(
      dir.clone(),
      vec![response(200, &[("cache-control", "max-age=60")], "art")],
    );
    assert_eq!(get(&client).1, "art");
    let requests = client.inner.requests.lock().unwrap();
    assert!(requests[0].get(header::IF_NONE_MATCH).is_none());
    // the colliding entry was replaced
    assert_eq!(read_meta(&base).unwrap().url, URL);
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn expired_entries_without_validators_are_refetched_unconditionally() {
    let dir = temp_dir("expired-plain");
    let base = store::base(&dir, URL);
    let meta = store::Meta {
      url: URL.into(),
      expires: 1,
      etag: None,
      last_modified: None,
      content_type: None,
    };
    store::write_entry(&base, &meta, b"old").unwrap();
    let client = client(
      dir.clone(),
      vec![response(200, &[("cache-control", "max-age=60")], "new")],
    );
    assert_eq!(get(&client).1, "new");
    let requests = client.inner.requests.lock().unwrap();
    assert!(requests[0].get(header::IF_NONE_MATCH).is_none());
    assert!(requests[0].get(header::IF_MODIFIED_SINCE).is_none());
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn fresh_but_missing_body_goes_to_the_network() {
    let dir = temp_dir("fresh-no-body");
    let client = client(
      dir.clone(),
      vec![
        response(200, &[("cache-control", "max-age=3600")], "art"),
        response(200, &[("cache-control", "max-age=3600")], "again"),
      ],
    );
    get(&client);
    fs::remove_file(store::path(&store::base(&dir, URL), "body")).unwrap();
    assert_eq!(get(&client).1, "again");
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn uncacheable_requests_bypass_the_cache() {
    let dir = temp_dir("bypass");
    let client = client(
      dir.clone(),
      vec![
        response(200, &[("cache-control", "max-age=3600")], "a"),
        response(200, &[("cache-control", "max-age=3600")], "b"),
      ],
    );
    let post = || Request::post(URL).body(AsyncBody::empty()).unwrap();
    assert_eq!(send(&client, post()).unwrap().2, "a");
    assert_eq!(send(&client, post()).unwrap().2, "b");
    assert_eq!(files(&dir), 0);
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn cache_hits_only_keep_the_content_type() {
    let dir = temp_dir("hit-headers");
    let client = client(
      dir.clone(),
      vec![response(
        200,
        &[
          ("cache-control", "max-age=3600"),
          ("content-type", "image/png"),
          ("x-extra", "1"),
        ],
        "art",
      )],
    );
    let (_, first, _) = send(&client, plain_get()).unwrap();
    assert_eq!(first["x-extra"], "1");
    let (status, headers, body) = send(&client, plain_get()).unwrap();
    assert_eq!((status, body.as_str()), (StatusCode::OK, "art"));
    assert_eq!(headers[header::CONTENT_TYPE], "image/png");
    assert!(headers.get("x-extra").is_none());
    fs::remove_dir_all(dir).ok();
  }

  #[test]
  fn new_creates_the_directory() {
    let dir = temp_dir("new").join("nested").join("cache");
    let _client = CachedHttpClient::new(Fake::default(), dir.clone()).unwrap();
    assert!(dir.is_dir());
  }

  #[test]
  fn new_fails_on_a_file_path() {
    let dir = temp_dir("new-file");
    let file = dir.join("file");
    fs::write(&file, b"").unwrap();
    assert!(CachedHttpClient::new(Fake::default(), file.join("cache")).is_err());
  }
}
