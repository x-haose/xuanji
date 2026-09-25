//! `xuanji://localhost/...` 自定义协议：从编入二进制的 web 资源 serve 页面，
//! 并经 `/__local/<百分号编码的绝对路径>` 提供用户选的本地图片 / 视频。
//! 用自定义协议而非 `file://` —— WKWebView 下 `file://` 会因跨域拦掉相对 js；
//! 页面源也不允许加载 `file://` 子资源（两内核均报 Not allowed to load local resource）。
//!
//! 资源经 `rust-embed` 内嵌：release 打进二进制、debug 运行时读源码树（保留热更）。
//! `settings/node_modules` 排除在外（54M，非运行时所需）。

use std::borrow::Cow;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

use percent_encoding::percent_decode_str;
use rust_embed::RustEmbed;
use wry::RequestAsyncResponder;
use wry::http::{Request, Response, header};

/// 本地文件路由前缀（页面侧 `localFileUrl` 与之对应）。
const LOCAL_PREFIX: &str = "/__local/";

/// 单次 Range 响应的最大字节数：视频按段流式读取，不整文件进内存。
const MAX_RANGE_CHUNK: u64 = 4 * 1024 * 1024;

/// 协议入口：本地文件在后台线程读（大视频不阻塞事件循环），内嵌资源就地响应。
pub fn handle(request: Request<Vec<u8>>, responder: RequestAsyncResponder) {
    let Some(encoded) = request.uri().path().strip_prefix(LOCAL_PREFIX) else {
        responder.respond(serve(&request));
        return;
    };
    let encoded = encoded.to_owned();
    let range = request
        .headers()
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    std::thread::spawn(move || responder.respond(serve_local(&encoded, range.as_deref())));
}

/// web/ 资源集合。folder 相对 `CARGO_MANIFEST_DIR`。
#[derive(RustEmbed)]
#[folder = "web/"]
#[exclude = "settings/node_modules/*"]
struct Web;

/// 按 URL 路径读取内嵌 web 资源，返回 HTTP 响应。
/// 越界（`..` 穿越）或缺失一律 404，绝不越出 web 目录（debug fs 模式下同样成立）。
fn serve(request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
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
    status(404)
}

fn status(code: u16) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(code)
        .body(Cow::Borrowed(&b""[..]))
        .expect("status-only response is always valid")
}

/// 可经本地路由读取的文件类型（仅图片 / 视频）；其余扩展名返回 None。
/// 协议只服务本程序内嵌页面，限定类型即可，不需要逐文件白名单。
fn local_mime(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        _ => return None,
    })
}

/// 列出目录下（不递归）可作背景的图片，按文件名排序，供轮播背景使用。
pub fn list_images(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() && local_mime(&path).is_some_and(|m| m.starts_with("image/")) {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// 读本地图片 / 视频。带 `Range` 时回 206 分段（视频播放器按段取数），否则整文件 200。
/// 非绝对路径 / 非白名单类型 / 不存在 → 404；Range 越界 → 416；读盘失败 → 500。
fn serve_local(encoded: &str, range: Option<&str>) -> Response<Cow<'static, [u8]>> {
    let Ok(decoded) = percent_decode_str(encoded).decode_utf8() else {
        return status(400);
    };
    let path = PathBuf::from(decoded.as_ref());
    let Some(mime) = local_mime(&path).filter(|_| path.is_absolute()) else {
        return not_found();
    };
    match read_local(&path, range) {
        Ok(Some(part)) => {
            let mut b = Response::builder()
                .status(part.status)
                .header(header::CONTENT_TYPE, mime)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, part.body.len());
            if let Some(cr) = part.content_range {
                b = b.header(header::CONTENT_RANGE, cr);
            }
            b.body(Cow::Owned(part.body))
                .unwrap_or_else(|_| status(500))
        }
        Ok(None) => not_found(),
        Err(LocalReadError::Unsatisfiable(len)) => Response::builder()
            .status(416)
            .header(header::CONTENT_RANGE, format!("bytes */{len}"))
            .body(Cow::Borrowed(&b""[..]))
            .unwrap_or_else(|_| status(500)),
        Err(LocalReadError::Io(e)) => {
            eprintln!("[shell] 读本地文件失败 {}: {e}", path.display());
            status(500)
        }
    }
}

/// 读到的整文件（200）或其中一段（206）。
struct LocalPart {
    status: u16,
    body: Vec<u8>,
    /// 仅 206 有：`bytes start-end/len`。
    content_range: Option<String>,
}

enum LocalReadError {
    /// Range 超出文件长度（附文件长度，回 416）。
    Unsatisfiable(u64),
    Io(std::io::Error),
}

/// 读取文件（或其中一段）。`Ok(None)` = 文件不存在或不是普通文件。
fn read_local(path: &Path, range: Option<&str>) -> Result<Option<LocalPart>, LocalReadError> {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(LocalReadError::Io(e)),
    };
    let meta = file.metadata().map_err(LocalReadError::Io)?;
    if !meta.is_file() {
        return Ok(None);
    }
    let len = meta.len();
    let Some(spec) = range else {
        let mut body = Vec::with_capacity(len as usize);
        file.read_to_end(&mut body).map_err(LocalReadError::Io)?;
        return Ok(Some(LocalPart {
            status: 200,
            body,
            content_range: None,
        }));
    };
    let (start, end) = parse_range(spec, len).ok_or(LocalReadError::Unsatisfiable(len))?;
    let end = end.min(start + MAX_RANGE_CHUNK - 1);
    file.seek(SeekFrom::Start(start))
        .map_err(LocalReadError::Io)?;
    let mut body = vec![0; (end - start + 1) as usize];
    file.read_exact(&mut body).map_err(LocalReadError::Io)?;
    Ok(Some(LocalPart {
        status: 206,
        body,
        content_range: Some(format!("bytes {start}-{end}/{len}")),
    }))
}

/// 解析单段 `Range: bytes=…`（`a-b` / `a-` / `-n`），返回文件内闭区间 [start, end]。
/// 多段、格式错误、越界或空文件返回 None。
fn parse_range(spec: &str, len: u64) -> Option<(u64, u64)> {
    let (a, b) = spec.trim().strip_prefix("bytes=")?.split_once('-')?;
    if len == 0 || b.contains(',') {
        return None;
    }
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        (len.checked_sub(n.min(len))?, len - 1)
    } else {
        let start: u64 = a.parse().ok()?;
        let end = if b.is_empty() {
            len - 1
        } else {
            b.parse::<u64>().ok()?.min(len - 1)
        };
        (start, end)
    };
    (start <= end && start < len).then_some((start, end))
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
    fn parses_ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=0-1", 100), Some((0, 1)));
        assert_eq!(parse_range("bytes=10-500", 100), Some((10, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-500", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("bytes=5-2", 100), None);
        assert_eq!(parse_range("bytes=0-1,5-6", 100), None);
        assert_eq!(parse_range("bytes=0-", 0), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }

    #[test]
    fn local_types_are_media_only() {
        assert_eq!(local_mime(Path::new("/a/b.JPG")), Some("image/jpeg"));
        assert_eq!(local_mime(Path::new("/a/b.mp4")), Some("video/mp4"));
        assert_eq!(local_mime(Path::new("/etc/passwd")), None);
        assert_eq!(local_mime(Path::new("/a/b.html")), None);
    }

    #[test]
    fn index_html_is_embedded() {
        assert!(Web::get("index.html").is_some());
    }
}
