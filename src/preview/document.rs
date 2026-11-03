//! Local, read-only document snapshots. No UI objects cross worker threads.
use objc2::{
    AllocAnyThread, class, msg_send,
    rc::{Allocated, Retained, autoreleasepool},
    runtime::AnyObject,
};
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
use objc2_foundation::{NSData, NSDictionary, NSRect, NSSize};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
            process::CommandExt,
        },
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

#[link(name = "PDFKit", kind = "framework")]
unsafe extern "C" {}
const MAX_FILE: u64 = 250 * 1024 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Workdir(PathBuf);
impl Workdir {
    fn new() -> Result<Self, String> {
        for _ in 0..100 {
            let path = std::env::temp_dir()
                .canonicalize()
                .map_err(|e| e.to_string())?
                .join(format!(
                    "herdr-document-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
                        .map_err(|e| e.to_string())?;
                    return Ok(Self(path));
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.to_string()),
            }
        }
        Err("문서 임시 폴더를 만들 수 없습니다.".into())
    }
}
impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) struct Document {
    work: Workdir,
    pdf: Option<PathBuf>,
    images: Vec<PathBuf>,
    pages: usize,
    cache: Mutex<VecDeque<usize>>,
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}
pub(super) fn is_document(path: &Path) -> bool {
    matches!(extension(path).as_str(), "pdf" | "ppt" | "pptx" | "png")
}
fn check_cancel(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("문서 열기가 취소되었습니다.".into())
    } else {
        Ok(())
    }
}
fn open_source(source: &Path) -> Result<File, String> {
    if source
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'))
    {
        return Err("숨김 파일은 미리볼 수 없습니다.".into());
    }
    if !source.is_absolute() {
        return Err("절대 파일 경로를 사용하세요.".into());
    }
    let parts: Vec<_> = source
        .components()
        .filter(|c| !matches!(c, std::path::Component::RootDir))
        .collect();
    let mut file = File::open("/").map_err(|e| e.to_string())?;
    for (i, part) in parts.iter().enumerate() {
        let name = match part {
            std::path::Component::Normal(name) => *name,
            std::path::Component::ParentDir => std::ffi::OsStr::new(".."),
            std::path::Component::CurDir => continue,
            _ => return Err("파일 경로가 올바르지 않습니다.".into()),
        };
        let name = std::ffi::CString::new(name.as_bytes())
            .map_err(|_| "파일 경로가 올바르지 않습니다.")?;
        let flags = libc::O_RDONLY
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | libc::O_CLOEXEC
            | if i + 1 < parts.len() {
                libc::O_DIRECTORY
            } else {
                0
            };
        let fd = unsafe { libc::openat(file.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(format!(
                "파일을 열지 못했습니다: {}",
                std::io::Error::last_os_error()
            ));
        }
        file = unsafe { File::from_raw_fd(fd) };
    }
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.nlink() != 1 || metadata.len() > MAX_FILE {
        return Err(
            "일반 파일(250 MiB 이하)만 미리볼 수 있으며 링크 파일은 지원하지 않습니다.".into(),
        );
    }
    Ok(file)
}

pub(super) fn checked_source_path(source: &Path) -> Result<PathBuf, String> {
    // macOS's standard root aliases are OS-owned; preserve all other original
    // path segments so O_NOFOLLOW also protects the subsequent snapshot read.
    let source = [
        ("/tmp", "/private/tmp"),
        ("/var", "/private/var"),
        ("/etc", "/private/etc"),
    ]
    .iter()
    .find_map(|(alias, real)| {
        source
            .strip_prefix(alias)
            .ok()
            .map(|p| Path::new(real).join(p))
    })
    .unwrap_or_else(|| source.to_owned());
    open_source(&source)?;
    Ok(source)
}

fn snapshot(source: &Path, destination: &Path) -> Result<(), String> {
    let mut file = open_source(source)?;
    let mut output = File::create(destination).map_err(|e| e.to_string())?;
    let copied = std::io::copy(&mut (&mut file).take(MAX_FILE + 1), &mut output)
        .map_err(|e| e.to_string())?;
    if copied > MAX_FILE {
        return Err("파일 크기가 250 MiB를 초과합니다.".into());
    }
    Ok(())
}
fn open_pdf(path: &Path) -> Result<Retained<AnyObject>, String> {
    let data = NSData::with_bytes(&fs::read(path).map_err(|e| e.to_string())?);
    unsafe {
        let allocated: Allocated<AnyObject> = msg_send![class!(PDFDocument), alloc];
        let doc: Option<Retained<AnyObject>> = msg_send![allocated,initWithData:&*data];
        let doc = doc.ok_or("PDF를 읽지 못했습니다. 파일이 손상되었는지 확인하세요.")?;
        let locked: bool = msg_send![&*doc, isLocked];
        if locked {
            return Err("암호화된 PDF는 잠금을 해제한 사본으로 열어주세요.".into());
        }
        Ok(doc)
    }
}
fn converter() -> Result<PathBuf, String> {
    let mut paths = Vec::new();
    if let Some(path) = std::env::var_os("HERDR_PREVIEW_SOFFICE") {
        paths.push(PathBuf::from(path));
    }
    paths.push(PathBuf::from(
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ));
    if let Some(home) = std::env::var_os("HOME") {
        paths.push(PathBuf::from(home).join("Applications/LibreOffice.app/Contents/MacOS/soffice"));
    }
    paths.extend(["/opt/homebrew/bin/soffice", "/usr/local/bin/soffice"].map(PathBuf::from));
    paths.into_iter().find(|p|p.is_file()).ok_or_else(||"PPT 미리보기에는 LibreOffice가 필요합니다. LibreOffice를 응용 프로그램 폴더에 설치한 뒤 다시 열어주세요.".into())
}
fn convert(input: &Path, work: &Path, cancelled: &AtomicBool) -> Result<PathBuf, String> {
    check_cancel(cancelled)?;
    let profile = work.join("profile");
    fs::create_dir(&profile).map_err(|e| e.to_string())?;
    fs::create_dir(profile.join("user")).map_err(|e| e.to_string())?;
    // Separate profile: never attach to an open office session, never run document macros.
    fs::write(profile.join("user/registrymodifications.xcu"),r#"<?xml version="1.0"?><oor:items xmlns:oor="http://openoffice.org/2001/registry"><item oor:path="/org.openoffice.Office.Common/Security/Scripting"><prop oor:name="MacroSecurityLevel" oor:op="fuse"><value>3</value></prop></item></oor:items>"#).map_err(|e|e.to_string())?;
    let profile = url::Url::from_directory_path(&profile)
        .map_err(|_| "변환 프로필 경로가 올바르지 않습니다.")?;
    let mut child = Command::new(converter()?)
        .arg(format!("-env:UserInstallation={profile}"))
        .args([
            "--headless",
            "--nologo",
            "--nodefault",
            "--norestore",
            "--convert-to",
            "pdf:impress_pdf_Export",
            "--outdir",
        ])
        .arg(work)
        .arg(input)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("PPT 변환을 시작하지 못했습니다: {e}"))?;
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) || started.elapsed() > Duration::from_secs(120) {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(if cancelled.load(Ordering::Relaxed) {
                "문서 열기가 취소되었습니다."
            } else {
                "PPT 변환 시간이 초과되었습니다. 파일을 확인하고 다시 열어주세요."
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) => return Err("PPT를 PDF로 변환하지 못했습니다. 파일을 확인하세요.".into()),
            Ok(None) => std::thread::sleep(Duration::from_millis(80)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    }
    let pdf = work.join(input.file_stem().unwrap()).with_extension("pdf");
    if !pdf.is_file() || fs::metadata(&pdf).map_err(|e| e.to_string())?.len() == 0 {
        return Err("PPT 변환 결과가 없습니다. 지원되는 PPT/PPTX 파일인지 확인하세요.".into());
    }
    check_cancel(cancelled)?;
    Ok(pdf)
}
impl Document {
    pub(super) fn prepare(paths: &[PathBuf], cancelled: &AtomicBool) -> Result<Self, String> {
        check_cancel(cancelled)?;
        if paths.is_empty() {
            return Err("미리볼 파일을 선택하세요.".into());
        }
        if paths.len() > 1 && paths.iter().any(|p| extension(p) != "png") {
            return Err("여러 파일을 함께 열 때는 PNG 파일만 선택하세요.".into());
        }
        let work = Workdir::new()?;
        let mut images = Vec::new();
        let mut pdf = None;
        for (i, source) in paths.iter().enumerate() {
            check_cancel(cancelled)?;
            let ext = extension(source);
            if !is_document(source) {
                return Err("PPT·PPTX·PDF·PNG 파일을 선택하세요.".into());
            }
            let dest = work.0.join(format!("source-{i}.{ext}"));
            snapshot(source, &dest)?;
            if ext == "png" {
                let (w, h) =
                    image::image_dimensions(&dest).map_err(|_| "PNG 이미지를 읽지 못했습니다.")?;
                if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 64_000_000 {
                    return Err("PNG 이미지 크기가 너무 크거나 올바르지 않습니다.".into());
                }
                images.push(dest);
            } else {
                pdf = Some(if ext == "pdf" {
                    dest
                } else {
                    convert(&dest, &work.0, cancelled)?
                });
            }
        }
        let pages = if let Some(path) = &pdf {
            autoreleasepool(|_| -> Result<usize, String> {
                let doc = open_pdf(path)?;
                let count: usize = unsafe { msg_send![&*doc, pageCount] };
                if count == 0 {
                    Err("PDF에 표시할 페이지가 없습니다.".into())
                } else {
                    Ok(count)
                }
            })?
        } else {
            images.len()
        };
        check_cancel(cancelled)?;
        Ok(Self {
            work,
            pdf,
            images,
            pages,
            cache: Mutex::new(VecDeque::new()),
        })
    }
    pub(super) fn page_count(&self) -> usize {
        self.pages
    }
    pub(super) fn root(&self) -> &Path {
        &self.work.0
    }
    pub(super) fn render(&self, page: usize) -> Result<PathBuf, String> {
        if page == 0 || page > self.pages {
            return Err(format!("1–{} 사이의 페이지 번호를 입력하세요.", self.pages));
        }
        if self.pdf.is_none() {
            let path = self.images[page - 1].clone();
            image::open(&path).map_err(|_| "PNG 이미지를 읽지 못했습니다.")?;
            return Ok(path);
        }
        let mut cache = self
            .cache
            .lock()
            .map_err(|_| "페이지 캐시를 열지 못했습니다.")?;
        let output = self.work.0.join(format!("page-{page}.png"));
        if !output.is_file() {
            autoreleasepool(|_| -> Result<(), String> {
                let doc = open_pdf(self.pdf.as_ref().unwrap())?;
                unsafe {
                    let pdf_page: Option<Retained<AnyObject>> = msg_send![&*doc,pageAtIndex:page-1];
                    let pdf_page = pdf_page.ok_or("PDF 페이지를 읽지 못했습니다.")?;
                    let bounds: NSRect = msg_send![&*pdf_page,boundsForBox:0_i64];
                    let width = bounds.size.width;
                    let height = bounds.size.height;
                    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
                        return Err("PDF 페이지 크기가 올바르지 않습니다.".into());
                    }
                    let scale = (2048. / width.max(height)).min(2.);
                    let image: Option<Retained<NSImage>> = msg_send![&*pdf_page,thumbnailOfSize:NSSize::new((width*scale).max(1.),(height*scale).max(1.)), forBox:0_i64];
                    let image = image.ok_or("PDF 페이지를 렌더링하지 못했습니다.")?;
                    let tiff = image
                        .TIFFRepresentation()
                        .ok_or("페이지 이미지를 인코딩하지 못했습니다.")?;
                    let bitmap = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
                        .ok_or("페이지 이미지를 읽지 못했습니다.")?;
                    let png = bitmap
                        .representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                        .ok_or("PNG 인코딩에 실패했습니다.")?;
                    File::create(&output)
                        .and_then(|mut f| f.write_all(&png.to_vec()))
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })?;
        }
        cache.retain(|n| *n != page);
        cache.push_back(page);
        while cache.len() > 4 {
            if let Some(old) = cache.pop_front() {
                let _ = fs::remove_file(self.work.0.join(format!("page-{old}.png")));
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "herdr-doc-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn png(&self, name: &str, rgb: [u8; 3]) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            image::save_buffer(&path, &rgb.repeat(32 * 16), 32, 16, image::ColorType::Rgb8)
                .unwrap();
            path
        }
        fn pdf(&self) -> PathBuf {
            let mut objects = vec![
                "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
                "<< /Type /Pages /Count 3 /Kids [3 0 R 5 0 R 7 0 R] >>".to_owned(),
            ];
            for n in 0..3 {
                objects.push(format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 180] /Contents {} 0 R >>",
                    4 + n * 2
                ));
                let text = format!("{} 0 0 rg 0 0 320 180 re f", (n + 1) as f32 / 3.);
                objects.push(format!(
                    "<< /Length {} >>\nstream\n{}\nendstream",
                    text.len(),
                    text
                ));
            }
            let mut bytes = b"%PDF-1.4\n".to_vec();
            let mut offsets = vec![0];
            for (i, o) in objects.iter().enumerate() {
                offsets.push(bytes.len());
                bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
            }
            let xref = bytes.len();
            bytes.extend_from_slice(
                format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes(),
            );
            for offset in offsets.iter().skip(1) {
                bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            bytes.extend_from_slice(
                format!(
                    "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                    offsets.len()
                )
                .as_bytes(),
            );
            let path = self.0.join("pages.pdf");
            fs::write(&path, bytes).unwrap();
            path
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn explicit_png_selection_preserves_order_and_duplicate_basenames() {
        let f = Fixture::new();
        let red = f.png("a/same.png", [255, 0, 0]);
        let blue = f.png("b/same.png", [0, 0, 255]);
        f.png("unselected.png", [0, 255, 0]);
        let doc = Document::prepare(&[blue.clone(), red.clone()], &AtomicBool::new(false)).unwrap();
        assert_eq!(doc.page_count(), 2);
        assert_eq!(
            image::open(doc.render(1).unwrap())
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [0, 0, 255]
        );
        assert_eq!(
            image::open(doc.render(2).unwrap())
                .unwrap()
                .to_rgb8()
                .get_pixel(0, 0)
                .0,
            [255, 0, 0]
        );
        assert!(doc.render(0).is_err());
        assert!(doc.render(3).is_err());
        let root = doc.root().to_owned();
        drop(doc);
        assert!(!root.exists());
        assert!(red.exists() && blue.exists());
    }

    #[test]
    fn multipage_pdf_renders_only_requested_pages_without_changing_source() {
        let f = Fixture::new();
        let path = f.pdf();
        let original = fs::read(&path).unwrap();
        let doc = Document::prepare(&[path.clone()], &AtomicBool::new(false)).unwrap();
        assert_eq!(doc.page_count(), 3);
        assert!(!doc.root().join("page-3.png").exists());
        let first = image::open(doc.render(1).unwrap()).unwrap().to_rgb8();
        let last = image::open(doc.render(3).unwrap()).unwrap().to_rgb8();
        assert!(first.width() >= 320 && first.width() <= 2048 && first.height() <= 2048);
        assert!(first.get_pixel(20, 20).0[0] < last.get_pixel(20, 20).0[0]);
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn rejects_cancelled_mixed_empty_corrupt_and_linked_documents() {
        let f = Fixture::new();
        let png = f.png("ok.png", [0, 0, 0]);
        let pdf = f.pdf();
        assert!(Document::prepare(&[], &AtomicBool::new(false)).is_err());
        assert!(Document::prepare(&[png.clone()], &AtomicBool::new(true)).is_err());
        assert!(Document::prepare(&[png.clone(), pdf], &AtomicBool::new(false)).is_err());
        let bad = f.0.join("bad.pdf");
        fs::write(&bad, b"not PDF").unwrap();
        assert!(Document::prepare(&[bad], &AtomicBool::new(false)).is_err());
        let link = f.0.join("link.png");
        std::os::unix::fs::symlink(&png, &link).unwrap();
        assert!(Document::prepare(&[link], &AtomicBool::new(false)).is_err());
        let hard = f.0.join("hard.png");
        fs::hard_link(&png, &hard).unwrap();
        assert!(Document::prepare(&[hard], &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn refuses_a_symlink_in_the_selected_images_parent_path() {
        let f = Fixture::new();
        f.png("actual/file.png", [0, 0, 0]);
        std::os::unix::fs::symlink(f.0.join("actual"), f.0.join("alias")).unwrap();
        assert!(Document::prepare(&[f.0.join("alias/file.png")], &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn allows_explicit_images_in_hidden_parent_folders_but_not_hidden_files() {
        let f = Fixture::new();
        let path = f.png(".cache/result.png", [0, 0, 255]);
        let doc = Document::prepare(&[path], &AtomicBool::new(false)).unwrap();
        assert_eq!(doc.page_count(), 1);
        assert!(doc.render(1).is_ok());
        let hidden = f.png(".secret.png", [255, 0, 0]);
        assert!(Document::prepare(&[hidden], &AtomicBool::new(false)).is_err());
    }

    #[test]
    #[ignore = "requires a local LibreOffice installation"]
    fn ppt_and_pptx_are_converted_to_three_static_slides() {
        let f = Fixture::new();
        for (name, bytes) in [
            (
                "deck.pptx",
                include_bytes!("../../tests/fixtures/document-preview/three-slides.pptx")
                    .as_slice(),
            ),
            (
                "deck.ppt",
                include_bytes!("../../tests/fixtures/document-preview/three-slides.ppt").as_slice(),
            ),
        ] {
            let path = f.0.join(name);
            fs::write(&path, bytes).unwrap();
            let doc = Document::prepare(&[path.clone()], &AtomicBool::new(false)).unwrap();
            assert_eq!(doc.page_count(), 3);
            let last = image::open(doc.render(3).unwrap()).unwrap().to_rgb8();
            assert!(last.get_pixel(20, 20).0[2] > 200);
            assert_eq!(fs::read(path).unwrap(), bytes);
        }
    }
}
