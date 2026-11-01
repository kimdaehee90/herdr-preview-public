//! A loopback-only static server for one explicitly opened artifact directory.
//! No shell, writes, directory listings, dotfiles or paths outside that root.
use std::fs::File;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

pub struct Server {
    pub root: PathBuf,
    file: Arc<File>,
    address: SocketAddr,
    stop: Arc<AtomicBool>,
}

impl Server {
    pub fn start(root: &Path) -> std::io::Result<Self> {
        let root = root.canonicalize()?;
        let root_file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&root)?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        let address = listener.local_addr()?;
        let file = Arc::new(root_file);
        let serving = file.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        std::thread::Builder::new()
            .name("artifact-preview".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(mut stream) = stream {
                        let _ = serve(&mut stream, &serving, address, &stopping);
                    }
                }
            })?;
        Ok(Self {
            root,
            file,
            address,
            stop,
        })
    }

    pub fn url(&self, file: &Path) -> Result<String, String> {
        let file = file.canonicalize().map_err(|e| e.to_string())?;
        let relative = file
            .strip_prefix(&self.root)
            .map_err(|_| "File is outside the preview folder")?;
        let mut url = url::Url::parse(&format!("http://{}/", self.address)).unwrap();
        url.path_segments_mut()
            .unwrap()
            .extend(relative.iter().map(|s| s.to_string_lossy().into_owned()));
        Ok(url.into())
    }

    pub fn target_for_url(&self, value: &str) -> Option<PathBuf> {
        let url = url::Url::parse(value).ok()?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port() != Some(self.address.port())
        {
            return None;
        }
        let (_, relative) = open_file(&self.file, url.path())?;
        Some(self.root.join(relative))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Wake the blocking accept; this server belongs only to this view.
        let _ = TcpStream::connect_timeout(&self.address, Duration::from_millis(100));
    }
}

fn relative_path(encoded: &str) -> Option<PathBuf> {
    // Reject traversal before URL normalization, including encoded separators.
    let decoded = decode(encoded.split('?').next()?.split('#').next()?)?;
    let relative = Path::new(decoded.trim_start_matches('/'));
    if relative.components().any(|c| match c {
        Component::Normal(s) => s
            .to_str()
            .is_none_or(|s| s.starts_with('.') || s.contains('\\')),
        _ => true,
    }) {
        return None;
    }
    Some(relative.to_owned())
}

fn open_at(parent: &File, name: &std::ffi::OsStr, directory: bool) -> std::io::Result<File> {
    let name = std::ffi::CString::new(name.as_bytes())?;
    let flags = libc::O_RDONLY
        | libc::O_NOFOLLOW
        | libc::O_CLOEXEC
        | libc::O_NONBLOCK
        | if directory { libc::O_DIRECTORY } else { 0 };
    // Every segment is opened relative to an already held directory descriptor.
    // No symlink/parent replacement can redirect a later read outside that inode.
    let fd = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn open_file(root: &File, encoded: &str) -> Option<(File, PathBuf)> {
    let mut path = relative_path(encoded)?;
    let parts: Vec<_> = path
        .components()
        .map(|c| c.as_os_str().to_owned())
        .collect();
    let mut file = root.try_clone().ok()?;
    for (i, part) in parts.iter().enumerate() {
        file = open_at(&file, part, i + 1 < parts.len()).ok()?;
    }
    if file.metadata().ok()?.is_dir() {
        file = open_at(&file, std::ffi::OsStr::new("index.html"), false).ok()?;
        path.push("index.html");
    }
    let metadata = file.metadata().ok()?;
    // Canonical paths cannot detect hard links. Refuse multiply linked files,
    // and read the same held descriptor that was checked (never reopen by path).
    (metadata.is_file() && metadata.nlink() == 1).then_some((file, path))
}

fn decode(s: &str) -> Option<String> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            out.push(
                u8::from_str_radix(std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?, 16).ok()?,
            );
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok().filter(|s| !s.contains('\0'))
}

fn mime(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "txt" | "md" => "text/plain; charset=utf-8",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

fn serve(
    stream: &mut TcpStream,
    root: &File,
    address: SocketAddr,
    stop: &AtomicBool,
) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut request = Vec::new();
    let mut chunk = [0; 1024];
    while request.len() < 8192 {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..n]);
        if request.windows(4).any(|s| s == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&request);
    let mut fields = text.lines().next().unwrap_or("").split_whitespace();
    let method = fields.next().unwrap_or("");
    let target = fields.next().unwrap_or("");
    let expected = address.to_string();
    let headers: Vec<_> = text
        .lines()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .collect();
    let host = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("host"))
        .map(|(_, v)| v.trim());
    let origin = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("origin"))
        .map(|(_, v)| v.trim());
    let permitted =
        host == Some(expected.as_str()) && origin.is_none_or(|o| o == format!("http://{address}"));
    let (status, content_type, bytes) = if !permitted || stop.load(Ordering::Relaxed) {
        ("403 Forbidden", "text/plain", b"Forbidden".to_vec())
    } else if method != "GET" && method != "HEAD" {
        (
            "405 Method Not Allowed",
            "text/plain",
            b"Read-only preview".to_vec(),
        )
    } else if let Some((file, path)) = open_file(root, target) {
        match file.metadata().and_then(|m| {
            if m.len() <= 64 * 1024 * 1024 {
                let mut bytes = Vec::new();
                file.take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > 64 * 1024 * 1024 {
                    return Err(std::io::Error::other("too large"));
                }
                Ok(bytes)
            } else {
                Err(std::io::Error::other("too large"))
            }
        }) {
            Ok(bytes) => ("200 OK", mime(&path), bytes),
            Err(_) => (
                "413 Content Too Large",
                "text/plain",
                b"Cannot preview this file".to_vec(),
            ),
        }
    } else {
        ("404 Not Found", "text/plain", b"File not found".to_vec())
    };
    if stop.load(Ordering::Relaxed) {
        return Ok(());
    }
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nCross-Origin-Resource-Policy: same-origin\r\nConnection: close\r\n\r\n",
        bytes.len()
    )?;
    if method != "HEAD" {
        stream.write_all(&bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_files_and_http_assets() {
        let base = std::env::temp_dir().join(format!("herdr-preview-test-{}", std::process::id()));
        let root = base.join("public");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("한글 파일.html"), "<h1>preview</h1>").unwrap();
        std::fs::write(root.join(".secret"), "private").unwrap();
        std::fs::write(base.join("outside.txt"), "outside").unwrap();
        std::os::unix::fs::symlink(base.join("outside.txt"), root.join("escape.txt")).unwrap();
        let root = root.canonicalize().unwrap();
        let root_file = File::open(&root).unwrap();
        std::fs::hard_link(base.join("outside.txt"), root.join("hardlink.txt")).unwrap();
        // Conservative policy also rejects links wholly inside the root: an
        // outside link could otherwise be added concurrently with enumeration.
        std::fs::write(root.join("internal.txt"), "internal bytes").unwrap();
        std::fs::hard_link(root.join("internal.txt"), root.join("internal-link.txt")).unwrap();
        assert!(open_file(&root_file, "/%2e%2e/outside.txt").is_none());
        assert!(open_file(&root_file, "/.secret").is_none());
        assert!(open_file(&root_file, "/escape.txt").is_none());
        assert!(open_file(&root_file, "/hardlink.txt").is_none());
        assert!(open_file(&root_file, "/internal.txt").is_none());
        assert!(open_file(&root_file, "/internal-link.txt").is_none());
        assert!(open_file(&root_file, "/%00").is_none());
        std::fs::write(root.join("checked.txt"), "checked bytes").unwrap();
        let (mut held, _) = open_file(&root_file, "/checked.txt").unwrap();
        std::fs::remove_file(root.join("checked.txt")).unwrap();
        std::os::unix::fs::symlink(base.join("outside.txt"), root.join("checked.txt")).unwrap();
        let mut held_bytes = String::new();
        held.read_to_string(&mut held_bytes).unwrap();
        assert_eq!(held_bytes, "checked bytes");
        assert!(open_file(&root_file, "/checked.txt").is_none());
        std::fs::create_dir(root.join("nested")).unwrap();
        std::fs::write(root.join("nested/ok.txt"), "nested safe").unwrap();
        assert!(open_file(&root_file, "/nested/ok.txt").is_some());
        std::fs::rename(root.join("nested"), root.join("held-nested")).unwrap();
        std::os::unix::fs::symlink(&base, root.join("nested")).unwrap();
        assert!(open_file(&root_file, "/nested/outside.txt").is_none());
        let server = Server::start(&root).unwrap();
        let url = url::Url::parse(&server.url(&root.join("한글 파일.html")).unwrap()).unwrap();
        let fetch = |path: &str, host: &str, extra: &str| {
            let mut stream = TcpStream::connect(server.address).unwrap();
            write!(stream, "GET {path} HTTP/1.1\r\nHost: {host}\r\n{extra}\r\n").unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            reply
        };
        assert!(fetch(url.path(), &server.address.to_string(), "").contains("<h1>preview</h1>"));
        assert!(fetch("/hardlink.txt", &server.address.to_string(), "").contains("404 Not Found"));
        assert!(fetch(url.path(), "evil.example", "").contains("403 Forbidden"));
        assert!(
            fetch(
                url.path(),
                &server.address.to_string(),
                "Origin: https://evil.example\r\n"
            )
            .contains("403 Forbidden")
        );
        drop(server);
        std::fs::remove_dir_all(base).unwrap();
    }
}
