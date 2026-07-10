//! `xuanji://localhost/...` 自定义协议：从编入二进制的 web 资源 serve 页面。
//! 用自定义协议而非 `file://` —— WKWebView 下 `file://` 会因跨域拦掉相对 js。
//!
//! 资源经 `rust-embed` 内嵌：release 打进二进制、debug 运行时读源码树（保留热更）。
//! `settings/node_modules` 排除在外（54M，非运行时所需）。

use std::borrow::Cow;
use std::path::{Component, Path};

use rust_embed::RustEmbed;
use wry::http::{Request, Response};

/// web/ 资源集合。folder 相对 `CARGO_MANIFEST_DIR`。
#[derive(RustEmbed)]
#[folder = "web/"]
#[exclude = "settings/node_modules/*"]
struct Web;

/// 按 URL 路径读取内嵌 web 资源，返回 HTTP 响应。
/// 越界（`..` 穿越）或缺失一律 404，绝不越出 web 目录（debug fs 模式下同样成立）。
pub fn serve(request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let raw = request.uri().path().trim_start_matches('/');
    let raw = if raw.is_empty() { "index.html" } else { raw };

    match normalize(raw) {
        Some(rel) => match Web::get(&rel) {
            Some(file) => Response::builder()
                .status(200)
                .header("Content-Type", mime_for(&rel))
                .header("Access-Control-Allow-Origin", "*")
                .body(file.data)
                .unwrap_or_else(|_| not_found()),
            None => not_found(),
        },
        None => not_found(),
    }
}

/// 把 URL 相对路径规范为内嵌 key（正斜杠分隔，无 `..`/绝对段）；任何逃逸段返回 None。
fn normalize(rel: &str) -> Option<String> {
    let mut segs = Vec::new();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(seg) => segs.push(seg.to_str()?),
            // 拒绝绝对路径、`..`、盘符等一切非普通段
            _ => return None,
        }
    }
    (!segs.is_empty()).then(|| segs.join("/"))
}

/// 按扩展名给 Content-Type；未知类型退回八位字节流。
fn mime_for(rel: &str) -> &'static str {
    match Path::new(rel).extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("mp4") => "video/mp4",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        _ => "application/octet-stream",
    }
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(404)
        .body(Cow::Borrowed(&b"not found"[..]))
        .expect("static 404 response is always valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        assert!(normalize("../etc/passwd").is_none());
        assert!(normalize("a/../../etc").is_none());
        assert!(normalize("/etc/passwd").is_none());
        assert!(normalize("").is_none());
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize("index.html").as_deref(), Some("index.html"));
        assert_eq!(
            normalize("assets/js/lunar.js").as_deref(),
            Some("assets/js/lunar.js")
        );
    }

    #[test]
    fn index_html_is_embedded() {
        assert!(Web::get("index.html").is_some());
    }
}
