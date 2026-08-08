// @author kongweiguang

//! HTTP client integration used by remote image loading.

#[cfg(test)]
pub(crate) mod update;
pub(crate) mod update_v2;

use std::io::{self, Read};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

use futures::AsyncReadExt;
use futures::FutureExt;
use futures::channel::oneshot;
use gpui::App;
use gpui::http_client::{self, AsyncBody, HttpClient};
use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CACHE_CONTROL, COOKIE, HeaderMap, HeaderName,
    HeaderValue, PRAGMA, REFERER, USER_AGENT,
};

const DEFAULT_IMAGE_ACCEPT: &str =
    "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
const DEFAULT_ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en-US;q=0.8,en;q=0.7";
const DEFAULT_CACHE_CONTROL: &str = "no-cache";
const REMOTE_IMAGE_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const REMOTE_IMAGE_TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
const REMOTE_IMAGE_MAX_REDIRECTS: usize = 5;
const REMOTE_IMAGE_MAX_RESPONSE_BYTES: usize = 20 * 1024 * 1024;
const REMOTE_IMAGE_MAX_CONCURRENT_REQUESTS: usize = 6;

static REMOTE_IMAGE_REQUEST_LIMITER: OnceLock<Arc<ImageRequestLimiter>> = OnceLock::new();

struct ImageRequestLimiter {
    available: Mutex<usize>,
    wake: Condvar,
}

struct ImageRequestPermit {
    limiter: Arc<ImageRequestLimiter>,
}

impl ImageRequestLimiter {
    fn acquire(self: &Arc<Self>) -> ImageRequestPermit {
        let mut available = match self.available.lock() {
            Ok(available) => available,
            Err(poisoned) => poisoned.into_inner(),
        };
        while *available == 0 {
            available = match self.wake.wait(available) {
                Ok(available) => available,
                Err(poisoned) => poisoned.into_inner(),
            };
        }
        *available -= 1;
        ImageRequestPermit {
            limiter: Arc::clone(self),
        }
    }
}

impl Drop for ImageRequestPermit {
    fn drop(&mut self) {
        let mut available = match self.limiter.available.lock() {
            Ok(available) => available,
            Err(poisoned) => poisoned.into_inner(),
        };
        *available = available
            .saturating_add(1)
            .min(REMOTE_IMAGE_MAX_CONCURRENT_REQUESTS);
        self.limiter.wake.notify_one();
    }
}

fn remote_image_request_limiter() -> Arc<ImageRequestLimiter> {
    REMOTE_IMAGE_REQUEST_LIMITER
        .get_or_init(|| {
            Arc::new(ImageRequestLimiter {
                available: Mutex::new(REMOTE_IMAGE_MAX_CONCURRENT_REQUESTS),
                wake: Condvar::new(),
            })
        })
        .clone()
}

pub(crate) fn install_http_client(cx: &mut App) {
    match ReqwestTransportHttpClient::new(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/147.0.0.0 Safari/537.36",
    ) {
        Ok(client) => cx.set_http_client(Arc::new(client)),
        Err(error) => eprintln!("failed to install HTTP client for image loading: {error}"),
    }
}

pub(crate) fn is_remote_image_source(source: &str) -> bool {
    parse_remote_http_url(source).is_ok()
}

fn parse_remote_http_url(source: &str) -> anyhow::Result<reqwest::Url> {
    let url = reqwest::Url::parse(source)
        .map_err(|error| anyhow::anyhow!("invalid remote image URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("remote image requests require an HTTP(S) URL");
    }
    if !url.username().is_empty() || url.password().is_some() {
        anyhow::bail!("remote image URLs must not include credentials");
    }
    Ok(url)
}

/// GPUI `HttpClient` bridge backed by reqwest's blocking transport.
///
/// GPUI expects an async client interface, while image loading in this app only
/// needs simple HTTP(S) fetches. Requests are executed on a short-lived thread
/// and returned as `AsyncBody` values to match GPUI's contract.
struct ReqwestTransportHttpClient {
    client: reqwest::blocking::Client,
    user_agent: HeaderValue,
    default_headers: HeaderMap,
}

impl ReqwestTransportHttpClient {
    fn new(user_agent: &str) -> anyhow::Result<Self> {
        let default_headers = default_image_request_headers(user_agent)?;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(REMOTE_IMAGE_CONNECT_TIMEOUT)
            .timeout(REMOTE_IMAGE_TOTAL_TIMEOUT)
            .redirect(remote_image_redirect_policy())
            .referer(false)
            .user_agent(user_agent)
            .default_headers(default_headers.clone())
            .build()?;
        Ok(Self {
            client,
            user_agent: HeaderValue::from_str(user_agent)?,
            default_headers,
        })
    }

    fn execute_request(
        client: reqwest::blocking::Client,
        default_headers: HeaderMap,
        request: http_client::Request<AsyncBody>,
    ) -> anyhow::Result<http_client::Response<AsyncBody>> {
        let (parts, mut body) = request.into_parts();
        let method = reqwest::Method::from_bytes(parts.method.as_str().as_bytes())?;
        let url = parse_remote_http_url(&parts.uri.to_string())?;
        let _permit = remote_image_request_limiter().acquire();
        let body_bytes = futures::executor::block_on(async move {
            let mut bytes = Vec::new();
            body.read_to_end(&mut bytes).await?;
            Ok::<Vec<u8>, io::Error>(bytes)
        })?;

        let mut builder = apply_missing_default_headers(
            client.request(method, url),
            &parts.headers,
            &default_headers,
        );
        if !body_bytes.is_empty() {
            builder = builder.body(body_bytes);
        }

        let response = builder.send()?;
        let status = response.status();
        let version = response.version();
        let headers = response.headers().clone();
        let bytes = read_response_body(response)?;

        let mut response_builder = http_client::Response::builder()
            .status(status)
            .version(version);
        for (name, value) in &headers {
            response_builder = response_builder.header(name, value);
        }
        Ok(response_builder.body(AsyncBody::from(bytes))?)
    }
}

fn remote_image_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= REMOTE_IMAGE_MAX_REDIRECTS {
            return attempt.error(anyhow::anyhow!(
                "remote image redirect limit exceeded (maximum {REMOTE_IMAGE_MAX_REDIRECTS})"
            ));
        }
        if let Err(error) = parse_remote_http_url(attempt.url().as_str()) {
            return attempt.error(error);
        }
        attempt.follow()
    })
}

fn read_response_body(mut response: reqwest::blocking::Response) -> anyhow::Result<Vec<u8>> {
    let content_length = response.content_length();
    read_limited_body(&mut response, content_length)
}

fn read_limited_body<R: Read>(
    mut reader: R,
    content_length: Option<u64>,
) -> anyhow::Result<Vec<u8>> {
    if content_length.is_some_and(|length| length > REMOTE_IMAGE_MAX_RESPONSE_BYTES as u64) {
        anyhow::bail!(
            "remote image response body exceeds the {} MiB limit",
            REMOTE_IMAGE_MAX_RESPONSE_BYTES / (1024 * 1024)
        );
    }

    let capacity = content_length
        .map(|length| length.min(REMOTE_IMAGE_MAX_RESPONSE_BYTES as u64) as usize)
        .unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }

        let next_len = bytes
            .len()
            .checked_add(read)
            .ok_or_else(|| anyhow::anyhow!("remote image response body size overflowed"))?;
        if next_len > REMOTE_IMAGE_MAX_RESPONSE_BYTES {
            anyhow::bail!(
                "remote image response body exceeds the {} MiB limit",
                REMOTE_IMAGE_MAX_RESPONSE_BYTES / (1024 * 1024)
            );
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(bytes)
}

fn default_image_request_headers(user_agent: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_str(user_agent)?);
    headers.insert(ACCEPT, HeaderValue::from_static(DEFAULT_IMAGE_ACCEPT));
    headers.insert(
        ACCEPT_LANGUAGE,
        HeaderValue::from_static(DEFAULT_ACCEPT_LANGUAGE),
    );
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(DEFAULT_CACHE_CONTROL),
    );
    headers.insert(PRAGMA, HeaderValue::from_static(DEFAULT_CACHE_CONTROL));
    Ok(headers)
}

fn apply_missing_default_headers(
    builder: reqwest::blocking::RequestBuilder,
    request_headers: &HeaderMap,
    default_headers: &HeaderMap,
) -> reqwest::blocking::RequestBuilder {
    let mut headers = request_headers.clone();
    for name in [COOKIE, AUTHORIZATION, REFERER] {
        headers.remove(name);
    }
    for (name, value) in default_headers {
        if !is_restricted_image_request_header(name) && !headers.contains_key(name) {
            headers.insert(name.clone(), value.clone());
        }
    }
    builder.headers(headers)
}

fn is_restricted_image_request_header(name: &HeaderName) -> bool {
    name == COOKIE || name == AUTHORIZATION || name == REFERER
}

impl HttpClient for ReqwestTransportHttpClient {
    fn type_name(&self) -> &'static str {
        "gmark_reqwest_transport_http_client"
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn send(
        &self,
        request: http_client::Request<AsyncBody>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<http_client::Response<AsyncBody>>> {
        let client = self.client.clone();
        let default_headers = self.default_headers.clone();
        let (tx, rx) = oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(Self::execute_request(client, default_headers, request));
        });
        async move {
            rx.await
                .map_err(|_| anyhow::anyhow!("image HTTP worker dropped before responding"))?
        }
        .boxed()
    }

    fn proxy(&self) -> Option<&http_client::Url> {
        None
    }
}

#[cfg(test)]
#[path = "../../tests/unit/net/tests.rs"]
mod tests;
