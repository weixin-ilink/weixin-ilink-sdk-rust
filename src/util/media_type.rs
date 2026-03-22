use std::path::Path;

static EXTENSION_MAP: &[(&str, &str)] = &[
    (".pdf", "application/pdf"),
    (".doc", "application/msword"),
    (".docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    (".xls", "application/vnd.ms-excel"),
    (".xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    (".ppt", "application/vnd.ms-powerpoint"),
    (".pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation"),
    (".txt", "text/plain"),
    (".csv", "text/csv"),
    (".zip", "application/zip"),
    (".tar", "application/x-tar"),
    (".gz", "application/gzip"),
    (".mp3", "audio/mpeg"),
    (".ogg", "audio/ogg"),
    (".wav", "audio/wav"),
    (".mp4", "video/mp4"),
    (".mov", "video/quicktime"),
    (".webm", "video/webm"),
    (".mkv", "video/x-matroska"),
    (".avi", "video/x-msvideo"),
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
    (".bmp", "image/bmp"),
];

static MIME_TO_EXT: &[(&str, &str)] = &[
    ("image/jpeg", ".jpg"),
    ("image/png", ".png"),
    ("image/gif", ".gif"),
    ("image/webp", ".webp"),
    ("image/bmp", ".bmp"),
    ("video/mp4", ".mp4"),
    ("video/quicktime", ".mov"),
    ("video/webm", ".webm"),
    ("audio/mpeg", ".mp3"),
    ("audio/ogg", ".ogg"),
    ("audio/wav", ".wav"),
    ("application/pdf", ".pdf"),
    ("application/zip", ".zip"),
    ("text/plain", ".txt"),
];

/// Get MIME type from file path extension.
pub fn mime_from_path(path: &Path) -> &'static str {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        for &(e, mime) in EXTENSION_MAP {
            // EXTENSION_MAP keys start with '.', skip it for comparison.
            if e[1..].eq_ignore_ascii_case(ext) {
                return mime;
            }
        }
    }
    "application/octet-stream"
}

/// Get file extension from MIME type.
pub fn extension_from_mime(mime: &str) -> &'static str {
    let ct = mime.split(';').next().unwrap_or("").trim();
    for &(m, ext) in MIME_TO_EXT {
        if m.eq_ignore_ascii_case(ct) {
            return ext;
        }
    }
    ".bin"
}

/// Get file extension from Content-Type header or URL path.
pub fn extension_from_content_type_or_url(content_type: Option<&str>, url: &str) -> &'static str {
    if let Some(ct) = content_type {
        let ext = extension_from_mime(ct);
        if ext != ".bin" {
            return ext;
        }
    }
    // Try extracting from URL path.
    if let Ok(parsed) = url::Url::parse(url) {
        let path = parsed.path();
        if let Some(dot) = path.rfind('.') {
            let ext = &path[dot..];
            for &(e, _) in EXTENSION_MAP {
                if e.eq_ignore_ascii_case(ext) {
                    return e;
                }
            }
        }
    }
    ".bin"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_mime_from_path() {
        assert_eq!(mime_from_path(Path::new("photo.jpg")), "image/jpeg");
        assert_eq!(mime_from_path(Path::new("video.MP4")), "video/mp4");
        assert_eq!(mime_from_path(Path::new("doc.pdf")), "application/pdf");
        assert_eq!(mime_from_path(Path::new("unknown.xyz")), "application/octet-stream");
    }

    #[test]
    fn test_extension_from_mime() {
        assert_eq!(extension_from_mime("image/jpeg"), ".jpg");
        assert_eq!(extension_from_mime("video/mp4; charset=utf-8"), ".mp4");
        assert_eq!(extension_from_mime("application/unknown"), ".bin");
    }
}
