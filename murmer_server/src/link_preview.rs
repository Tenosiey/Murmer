//! OpenGraph link preview endpoint.
//!
//! `GET /link-preview?url=<http(s) url>` fetches the target page server-side
//! and returns the OpenGraph metadata (title, description, image, site name)
//! as JSON. Fetching on the server keeps client IPs hidden from linked sites
//! and works for pages that forbid framing.
//!
//! Because the server fetches attacker-supplied URLs, requests are restricted
//! to public IP addresses on default HTTP(S) ports: the hostname is resolved
//! first, every resolved address must be public, and the connection is pinned
//! to the vetted address so a DNS rebind cannot redirect the request into the
//! local network. Redirects are followed manually and re-vetted per hop.

use axum::{
    extract::Query,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use reqwest::{redirect, Url};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tracing::debug;

/// Maximum HTML bytes read from the target page.
const MAX_BODY_BYTES: usize = 512 * 1024;
/// Timeout for each HTTP request to the target.
const FETCH_TIMEOUT: Duration = Duration::from_secs(6);
/// Maximum redirects followed (each hop is re-vetted).
const MAX_REDIRECTS: usize = 3;
/// How long a cached preview stays valid.
const CACHE_TTL: Duration = Duration::from_secs(10 * 60);
/// Upper bound on cached entries; the cache is cleared when exceeded.
const CACHE_MAX_ENTRIES: usize = 512;
/// Upper bounds on returned field lengths (characters).
const MAX_TITLE_LEN: usize = 300;
const MAX_DESCRIPTION_LEN: usize = 500;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    site_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
}

#[derive(Deserialize)]
pub struct PreviewQuery {
    url: String,
}

fn cache() -> &'static Mutex<HashMap<String, (Instant, Preview)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Preview)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[tracing::instrument(skip_all, fields(url = %q.url))]
pub async fn link_preview(Query(q): Query<PreviewQuery>) -> Response {
    let url = match validate_url(&q.url) {
        Ok(url) => url,
        Err(reason) => {
            debug!(%reason, "rejected link preview request");
            return StatusCode::UNPROCESSABLE_ENTITY.into_response();
        }
    };

    let key = url.to_string();
    if let Some(hit) = {
        let cache = cache().lock().unwrap();
        cache
            .get(&key)
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, preview)| preview.clone())
    } {
        return Json(hit).into_response();
    }

    match fetch_preview(url).await {
        Ok(preview) => {
            let mut cache = cache().lock().unwrap();
            if cache.len() >= CACHE_MAX_ENTRIES {
                cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
                if cache.len() >= CACHE_MAX_ENTRIES {
                    cache.clear();
                }
            }
            cache.insert(key, (Instant::now(), preview.clone()));
            Json(preview).into_response()
        }
        Err(reason) => {
            debug!(%reason, "link preview fetch failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Parse and vet a user-supplied URL: http(s) only, no credentials, default
/// ports only (blocks internal port scanning).
fn validate_url(raw: &str) -> Result<Url, &'static str> {
    let url = Url::parse(raw).map_err(|_| "invalid URL")?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("unsupported scheme");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("credentials not allowed");
    }
    if url.port().is_some() {
        return Err("non-default port not allowed");
    }
    if url.host_str().is_none() {
        return Err("missing host");
    }
    Ok(url)
}

/// Resolve the URL's host and require every address to be public. Returns the
/// vetted address so the connection can be pinned to it.
async fn resolve_public(url: &Url) -> Result<SocketAddr, &'static str> {
    let host = url.host_str().ok_or("missing host")?;
    let port = url.port_or_known_default().unwrap_or(443);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| "DNS resolution failed")?
        .collect();
    if addrs.is_empty() {
        return Err("host did not resolve");
    }
    for addr in &addrs {
        if !is_public_ip(addr.ip()) {
            return Err("host resolves to a non-public address");
        }
    }
    Ok(addrs[0])
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(mapped) => is_public_v4(mapped),
            None => is_public_v6(v6),
        },
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_multicast()
        || ip.is_documentation()
        || octets[0] == 0
        // 100.64.0.0/10 (carrier-grade NAT)
        || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64)
        // 192.0.0.0/24 (IETF protocol assignments)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        // 198.18.0.0/15 (benchmarking)
        || (octets[0] == 198 && (octets[1] & 0b1111_1110) == 18)
        // 240.0.0.0/4 (reserved)
        || octets[0] >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        // fc00::/7 (unique local)
        || (segments[0] & 0xfe00) == 0xfc00
        // fe80::/10 (link-local unicast)
        || (segments[0] & 0xffc0) == 0xfe80)
}

/// Fetch the page, following redirects manually so every hop is re-vetted.
async fn fetch_preview(mut url: Url) -> Result<Preview, String> {
    for _ in 0..=MAX_REDIRECTS {
        let addr = resolve_public(&url).await.map_err(str::to_string)?;
        let host = url.host_str().ok_or("missing host")?.to_string();
        let client = reqwest::Client::builder()
            .redirect(redirect::Policy::none())
            .timeout(FETCH_TIMEOUT)
            .resolve(&host, addr)
            .user_agent("Mozilla/5.0 (compatible; MurmerBot/1.0; link preview)")
            .build()
            .map_err(|e| e.to_string())?;

        let response = client
            .get(url.clone())
            .header(header::ACCEPT, "text/html")
            .send()
            .await
            .map_err(|e| e.to_string())?;

        if response.status().is_redirection() {
            let location = response
                .headers()
                .get(header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or("redirect without location")?;
            url = url.join(location).map_err(|_| "invalid redirect target")?;
            validate_url(url.as_str()).map_err(str::to_string)?;
            continue;
        }

        if !response.status().is_success() {
            return Err(format!("target returned status {}", response.status()));
        }

        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type.starts_with("text/html")
            && !content_type.starts_with("application/xhtml+xml")
        {
            return Err(format!("unsupported content type '{content_type}'"));
        }

        let html = read_limited(response).await?;
        return Ok(extract_preview(&html, &url));
    }
    Err("too many redirects".to_string())
}

/// Read at most `MAX_BODY_BYTES` of the response body as a lossy UTF-8 string.
async fn read_limited(mut response: reqwest::Response) -> Result<String, String> {
    let mut body: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        let remaining = MAX_BODY_BYTES - body.len();
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if body.len() >= MAX_BODY_BYTES {
            break;
        }
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

/// Pull OpenGraph (with Twitter-card and plain HTML fallbacks) metadata out
/// of an HTML document.
fn extract_preview(html: &str, url: &Url) -> Preview {
    let metas = collect_meta_tags(html);
    let get = |names: &[&str]| -> Option<String> {
        for name in names {
            if let Some(content) = metas.get(*name) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    };

    let title = get(&["og:title", "twitter:title"])
        .or_else(|| html_title(html))
        .map(|t| truncate_chars(&t, MAX_TITLE_LEN));
    let description = get(&["og:description", "twitter:description", "description"])
        .map(|d| truncate_chars(&d, MAX_DESCRIPTION_LEN));
    let site_name = get(&["og:site_name"]).map(|s| truncate_chars(&s, MAX_TITLE_LEN));
    let image = get(&["og:image", "og:image:url", "twitter:image"])
        .and_then(|src| url.join(&src).ok())
        .filter(|u| u.scheme() == "http" || u.scheme() == "https")
        .map(|u| u.to_string());

    Preview {
        url: url.to_string(),
        site_name,
        title,
        description,
        image,
    }
}

/// Collect `<meta>` tags into a map keyed by their `property` or `name`
/// attribute (lowercased). First occurrence wins, matching how OpenGraph
/// consumers behave.
fn collect_meta_tags(html: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let lower = html.to_lowercase();
    let mut pos = 0;
    while let Some(start) = lower[pos..].find("<meta") {
        let tag_start = pos + start;
        let Some(end) = lower[tag_start..].find('>') else {
            break;
        };
        let tag = &html[tag_start..tag_start + end];
        let attrs = parse_attributes(tag);
        let key = attrs
            .get("property")
            .or_else(|| attrs.get("name"))
            .map(|k| k.to_lowercase());
        if let (Some(key), Some(content)) = (key, attrs.get("content")) {
            map.entry(key).or_insert_with(|| content.clone());
        }
        pos = tag_start + end;
    }
    map
}

/// Extract the text of the first `<title>` element.
fn html_title(html: &str) -> Option<String> {
    let lower = html.to_lowercase();
    let start = lower.find("<title")?;
    let open_end = lower[start..].find('>')? + start + 1;
    let close = lower[open_end..].find("</title")? + open_end;
    let title = decode_entities(html[open_end..close].trim());
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

/// Parse `name="value"` attributes from a single HTML tag. Values are
/// entity-decoded; attribute names are lowercased.
fn parse_attributes(tag: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let bytes = tag.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip to the start of an attribute name.
        if !bytes[i].is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let name_start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-' || bytes[i] == b':')
        {
            i += 1;
        }
        let name = tag[name_start..i].to_lowercase();
        // Skip whitespace before '='.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            continue; // valueless attribute
        }
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let value = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let value_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            let value = &tag[value_start..i];
            i += 1;
            value
        } else {
            let value_start = i;
            while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            &tag[value_start..i]
        };
        attrs.insert(name, decode_entities(value));
    }
    attrs
}

/// Decode the HTML entities that commonly appear in meta content.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix('#')
                .and_then(|num| {
                    if let Some(hex) = num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        num.parse::<u32>().ok()
                    }
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let truncated: String = text.chars().take(max).collect();
        format!("{truncated}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_opengraph_metadata() {
        let html = r#"<html><head>
            <title>Fallback title</title>
            <meta property="og:title" content="Example &amp; Friends" />
            <meta property="og:description" content="A &quot;great&quot; page">
            <meta property="og:site_name" content="Example">
            <meta property="og:image" content="/img/cover.png">
        </head></html>"#;
        let url = Url::parse("https://example.com/post/1").unwrap();
        let preview = extract_preview(html, &url);
        assert_eq!(preview.title.as_deref(), Some("Example & Friends"));
        assert_eq!(preview.description.as_deref(), Some("A \"great\" page"));
        assert_eq!(preview.site_name.as_deref(), Some("Example"));
        assert_eq!(
            preview.image.as_deref(),
            Some("https://example.com/img/cover.png")
        );
    }

    #[test]
    fn falls_back_to_title_tag_and_description_meta() {
        let html = r#"<head><TITLE> Plain page </TITLE>
            <meta name="description" content="meta description"></head>"#;
        let url = Url::parse("https://example.com/").unwrap();
        let preview = extract_preview(html, &url);
        assert_eq!(preview.title.as_deref(), Some("Plain page"));
        assert_eq!(preview.description.as_deref(), Some("meta description"));
        assert!(preview.image.is_none());
    }

    #[test]
    fn ignores_javascript_image_urls() {
        let html = r#"<meta property="og:image" content="javascript:alert(1)">"#;
        let url = Url::parse("https://example.com/").unwrap();
        assert!(extract_preview(html, &url).image.is_none());
    }

    #[test]
    fn decodes_numeric_entities() {
        assert_eq!(decode_entities("a&#39;b&#x41;c"), "a'bAc");
        assert_eq!(decode_entities("no entities & done"), "no entities & done");
    }

    #[test]
    fn rejects_unsafe_urls() {
        assert!(validate_url("ftp://example.com/").is_err());
        assert!(validate_url("https://example.com:8080/").is_err());
        assert!(validate_url("https://user:pw@example.com/").is_err());
        assert!(validate_url("not a url").is_err());
        assert!(validate_url("https://example.com/page").is_ok());
    }

    /// Network-dependent smoke test for the full fetch path; run explicitly
    /// with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn fetches_real_page() {
        let url = Url::parse("https://example.com/").unwrap();
        let preview = fetch_preview(url).await.expect("fetch should succeed");
        assert!(preview.title.is_some());
    }

    #[test]
    fn blocks_private_addresses() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:192.168.1.1",
        ] {
            assert!(!is_public_ip(ip.parse().unwrap()), "{ip} should be blocked");
        }
        for ip in ["93.184.216.34", "2606:2800:220:1::1"] {
            assert!(is_public_ip(ip.parse().unwrap()), "{ip} should be public");
        }
    }
}
