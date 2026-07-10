//! `xuanji://localhost/...` 自定义协议：从本地 web 目录 serve 页面资源。
//! 用自定义协议而非 `file://` —— WKWebView 下 `file://` 会因跨域拦掉相对 js。

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};

use wry::http::{Request, Response};

/// 按 URL 路径读取 web 目录下的文件，返回 HTTP 响应。
/// 越界（`..` 穿越）或缺失一律 404，绝不泄漏 web 目录之外的文件。
pub fn serve(root: &Path, request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let rel = request.uri().path().trim_start_matches('/');
    let rel = if rel.is_empty() { "index.html" } else { rel };

    match resolve(root, rel) {
        Some(path) => match std::fs::read(&path) {
            Ok(bytes) => Response::builder()
                .status(200)
                .header("Content-Type", mime_for(&path))
                .header("Access-Control-Allow-Origin", "*")
                .body(Cow::Owned(bytes))
                .unwrap_or_else(|_| not_found()),
            Err(_) => not_found(),
        },
        None => not_found(),
    }
}

/// 将相对路径安全解析为 root 内的绝对路径；任何试图逃出 root 的路径返回 None。
fn resolve(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(seg) => out.push(seg),
            // 拒绝绝对路径、`..`、盘符等一切非普通段
            _ => return None,
        }
    }
    out.starts_with(root).then_some(out)
}

/// 按扩展名给 Content-Type；未知类型退回八位字节流。
fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
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
        let root = Path::new("/srv/web");
        assert!(resolve(root, "../etc/passwd").is_none());
        assert!(resolve(root, "a/../../etc").is_none());
        assert!(resolve(root, "/etc/passwd").is_none());
    }

    #[test]
    fn resolves_normal_paths() {
        let root = Path::new("/srv/web");
        assert_eq!(resolve(root, "index.html"), Some(root.join("index.html")));
        assert_eq!(
            resolve(root, "assets/js/lunar.js"),
            Some(root.join("assets/js/lunar.js"))
        );
    }
}
