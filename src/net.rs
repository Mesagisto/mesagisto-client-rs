use std::time::Duration;

// fixme add proxy
pub fn new_reqwest_client() -> reqwest::Client {
  use reqwest::header::{HeaderMap, CONNECTION};

  let mut headers = HeaderMap::new();
  headers.insert(CONNECTION, "keep-alive".parse().unwrap());

  let connect_timeout = Duration::from_secs(5);
  let timeout = connect_timeout + Duration::from_secs(12);

  reqwest::Client::builder()
    .connect_timeout(connect_timeout)
    .timeout(timeout)
    .tcp_nodelay(true)
    .default_headers(headers)
    .use_rustls_tls()
    .build()
    .unwrap()
}
