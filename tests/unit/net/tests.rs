// @author kongweiguang

use super::{
    DEFAULT_ACCEPT_LANGUAGE, DEFAULT_CACHE_CONTROL, DEFAULT_IMAGE_ACCEPT,
    REMOTE_IMAGE_MAX_RESPONSE_BYTES, apply_missing_default_headers, default_image_request_headers,
    is_remote_image_source, read_limited_body,
};
use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, AUTHORIZATION, CACHE_CONTROL, CONNECTION,
    CONTENT_LENGTH, COOKIE, HOST, HeaderMap, HeaderValue, PRAGMA, REFERER, USER_AGENT,
};
use std::io::Cursor;

const TEST_USER_AGENT: &str = "gmarkTest/1.0";

#[test]
fn default_image_headers_include_browser_like_fetch_context() {
    let headers = default_image_request_headers(TEST_USER_AGENT).expect("headers");

    assert_eq!(headers.get(USER_AGENT).unwrap(), TEST_USER_AGENT);
    assert_eq!(headers.get(ACCEPT).unwrap(), DEFAULT_IMAGE_ACCEPT);
    assert_eq!(
        headers.get(ACCEPT_LANGUAGE).unwrap(),
        DEFAULT_ACCEPT_LANGUAGE
    );
    assert_eq!(headers.get(CACHE_CONTROL).unwrap(), DEFAULT_CACHE_CONTROL);
    assert_eq!(headers.get(PRAGMA).unwrap(), DEFAULT_CACHE_CONTROL);
}

#[test]
fn default_image_headers_leave_transport_managed_headers_unset() {
    let headers = default_image_request_headers(TEST_USER_AGENT).expect("headers");

    assert!(!headers.contains_key(ACCEPT_ENCODING));
    assert!(!headers.contains_key(CONNECTION));
    assert!(!headers.contains_key(CONTENT_LENGTH));
    assert!(!headers.contains_key(HOST));
}

#[test]
fn explicit_request_headers_override_default_image_headers() {
    let defaults = default_image_request_headers(TEST_USER_AGENT).expect("headers");
    let mut request_headers = HeaderMap::new();
    request_headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    request_headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-GB"));

    let request = apply_missing_default_headers(
        reqwest::blocking::Client::new().get("https://example.com/image.png"),
        &request_headers,
        &defaults,
    )
    .build()
    .expect("request should build");
    let headers = request.headers();

    assert_eq!(headers.get(ACCEPT).unwrap(), "application/json");
    assert_eq!(headers.get(ACCEPT_LANGUAGE).unwrap(), "en-GB");
    assert_eq!(headers.get(USER_AGENT).unwrap(), TEST_USER_AGENT);
    assert_eq!(headers.get(CACHE_CONTROL).unwrap(), DEFAULT_CACHE_CONTROL);
    assert_eq!(headers.get(PRAGMA).unwrap(), DEFAULT_CACHE_CONTROL);
}

#[test]
fn restricted_request_headers_are_removed() {
    let mut defaults = HeaderMap::new();
    defaults.insert(COOKIE, HeaderValue::from_static("default-cookie"));
    defaults.insert(AUTHORIZATION, HeaderValue::from_static("default-auth"));
    defaults.insert(REFERER, HeaderValue::from_static("https://example.com"));

    let mut request_headers = HeaderMap::new();
    request_headers.insert(COOKIE, HeaderValue::from_static("request-cookie"));
    request_headers.insert(AUTHORIZATION, HeaderValue::from_static("request-auth"));
    request_headers.insert(REFERER, HeaderValue::from_static("https://request.example"));

    let request = apply_missing_default_headers(
        reqwest::blocking::Client::new().get("https://example.com/image.png"),
        &request_headers,
        &defaults,
    )
    .build()
    .expect("request should build");

    assert!(!request.headers().contains_key(COOKIE));
    assert!(!request.headers().contains_key(AUTHORIZATION));
    assert!(!request.headers().contains_key(REFERER));
}

#[test]
fn detects_remote_http_sources() {
    assert!(is_remote_image_source("https://example.com/image.png"));
    assert!(is_remote_image_source("http://example.com/image.gif"));
    assert!(is_remote_image_source("HTTPS://example.com/image.webp"));
    assert!(!is_remote_image_source("./image.png"));
    assert!(!is_remote_image_source("images/photo.jpg"));
    assert!(!is_remote_image_source("ftp://example.com/image.png"));
    assert!(!is_remote_image_source("file:///tmp/image.png"));
    assert!(!is_remote_image_source("data:image/png;base64,AAAA"));
    assert!(!is_remote_image_source(
        "https://user:password@example.com/image.png"
    ));
}

#[test]
fn response_body_limit_rejects_declared_oversize() {
    let result = read_limited_body(
        Cursor::new(Vec::<u8>::new()),
        Some(REMOTE_IMAGE_MAX_RESPONSE_BYTES as u64 + 1),
    );

    assert!(result.is_err());
}

#[test]
fn response_body_is_read_incrementally_within_limit() {
    let body = vec![0x5a; 32 * 1024];
    let result = read_limited_body(Cursor::new(body.clone()), Some(body.len() as u64))
        .expect("body should be read");

    assert_eq!(result, body);
}
