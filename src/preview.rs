//! Inspector previews: text with syntax highlighting, images, PDFs, folders, archives, media info.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use serde_json::{Value, json};
use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};
use syntect::util::LinesWithEndings;

use crate::listing;
use crate::preview_extra::*;
use crate::state::cache_dir;
use crate::theme::palette;

// A preview is a glance, not an editor: a few screens of text is plenty, and highlighting more costs real time.
const MAX_TEXT_BYTES: usize = 24 * 1024;
const MAX_TEXT_LINES: usize = 200;

/// Decoded by Qt directly.
const IMAGE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "ico"];
const FONT_EXT: &[&str] = &["ttf", "otf", "woff", "woff2", "ttc"];
const OFFICE_EXT: &[&str] = &["docx", "docm", "dotx", "xlsx", "xlsm", "pptx", "pptm", "odt", "ods", "odp"];
const COVER_EXT: &[&str] = &["epub", "cbz", "cbr"];
const ARCHIVE_EXT: &[&str] = &[
    "zip", "tar", "tgz", "gz", "xz", "bz2", "zst", "7z", "rar", "iso", "txz", "tbz2", "jar", "apk", "whl",
];
const AUDIO_EXT: &[&str] = &["mp3", "flac", "ogg", "opus", "wav", "m4a", "aac", "wma"];
const VIDEO_EXT: &[&str] = &["mp4", "mkv", "webm", "mov", "avi", "m4v", "wmv", "flv"];

/// Load the syntax definitions ahead of the first text preview (it takes about 60 ms).
pub fn warm_up() {
    syntaxes();
}

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn ext_of(path: &str) -> String {
    Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn html_escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            ' ' => out.push_str("&nbsp;"),
            '\t' => out.push_str("&nbsp;&nbsp;&nbsp;&nbsp;"),
            '\n' => out.push_str("<br>"),
            '\r' => {}
            c => out.push(c),
        }
    }
}

/// Map a TextMate scope stack to a palette colour key (innermost scope wins).
fn color_key(stack: &ScopeStack) -> &'static str {
    const RULES: &[(&str, &str)] = &[
        ("comment", "dark_foreground"),
        ("string", "green"),
        ("constant.numeric", "orange"),
        ("constant", "orange"),
        ("keyword", "magenta"),
        ("storage", "magenta"),
        ("entity.name.function", "blue"),
        ("support.function", "blue"),
        ("entity.name.tag", "red"),
        ("entity.other.attribute-name", "yellow"),
        ("entity.name", "yellow"),
        ("support", "yellow"),
        ("variable.parameter", "orange"),
        ("markup.heading", "blue"),
        ("markup.bold", "yellow"),
        ("markup.raw", "green"),
        ("invalid", "red"),
    ];
    for scope in stack.as_slice().iter().rev() {
        let name = scope_name(scope);
        if let Some((_, key)) = RULES.iter().find(|(prefix, _)| name.starts_with(prefix)) {
            return key;
        }
    }
    "foreground"
}

fn scope_name(scope: &Scope) -> String {
    scope.build_string()
}

fn highlight_html(text: &str, path: &str) -> String {
    let ss = syntaxes();
    let syntax = ss
        .find_syntax_by_extension(&ext_of(path))
        .or_else(|| {
            Path::new(path).file_name().and_then(|n| ss.find_syntax_by_extension(&n.to_string_lossy()))
        })
        .or_else(|| ss.find_syntax_by_first_line(text.lines().next().unwrap_or("")));
    let text = text.strip_suffix('\n').unwrap_or(text);
    let pal = palette();
    let color = |key: &str| pal.get(key).cloned().unwrap_or_else(|| "#cccccc".into());

    let mut out = String::with_capacity(text.len() * 2);
    let dim = color("dark_foreground");
    let mut line_no = 1usize;
    let number = |n: usize, out: &mut String| {
        let digits = n.to_string();
        let pad = "&nbsp;".repeat(3usize.saturating_sub(digits.len()));
        out.push_str(&format!("<span style=\"color:{dim}\">{pad}{digits}</span>&nbsp;&nbsp;"));
    };
    number(line_no, &mut out);
    // Emit `piece` in colour `key`, starting a numbered line after every newline (even inside a span).
    let mut emit = |piece: &str, key: &str, out: &mut String| {
        for (i, part) in piece.split('\n').enumerate() {
            if i > 0 {
                out.push_str("<br>");
                line_no += 1;
                number(line_no, out);
            }
            if part.is_empty() {
                continue;
            }
            if key == "foreground" {
                html_escape(part, out);
            } else {
                out.push_str(&format!("<span style=\"color:{}\">", color(key)));
                html_escape(part, out);
                out.push_str("</span>");
            }
        }
    };
    let Some(syntax) = syntax else {
        emit(text, "foreground", &mut out);
        return out;
    };
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    for line in LinesWithEndings::from(text) {
        let Ok(ops) = state.parse_line(line, ss) else {
            emit(line, "foreground", &mut out);
            continue;
        };
        let mut last = 0;
        for (pos, op) in ops {
            emit(&line[last..pos], color_key(&stack), &mut out);
            let _ = stack.apply(&op);
            last = pos;
        }
        emit(&line[last..], color_key(&stack), &mut out);
    }
    out
}

fn read_head(path: &str, max: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    fs::File::open(path).ok()?.take(max as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0) || std::str::from_utf8(bytes).is_err() && {
        // A multibyte char cut at the end of the buffer is not binary.
        let valid = std::str::from_utf8(bytes).err().map_or(0, |e| e.valid_up_to());
        bytes.len() - valid > 3
    }
}

fn cmd_lines(cmd: &str, args: &[&str], limit: usize) -> Vec<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).lines().take(limit).map(str::to_owned).collect())
        .unwrap_or_default()
}

fn pdf_page_png(path: &str, mtime: i64) -> Option<String> {
    let mut h = DefaultHasher::new();
    (path, mtime).hash(&mut h);
    let dir = cache_dir().join("pdf");
    fs::create_dir_all(&dir).ok()?;
    let stem = dir.join(format!("{:x}", h.finish()));
    let png = stem.with_extension("png");
    if !png.exists() {
        let ok = Command::new("pdftoppm")
            .args(["-png", "-r", "90", "-f", "1", "-l", "1", "-singlefile", path])
            .arg(&stem)
            .status()
            .ok()
            .is_some_and(|s| s.success());
        if !ok {
            return None;
        }
    }
    Some(png.to_string_lossy().into_owned())
}

fn fmt_duration(secs: f64) -> String {
    let total = secs.round() as i64;
    let (h, m, s) = (total / 3600, (total / 60) % 60, total % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// A few readable lines about an audio/video file, from ffprobe's JSON, plus its duration in seconds
/// and the picture size of its first video stream.
fn media_probe(path: &str) -> (Vec<String>, f64, Option<(i64, i64)>) {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_format", "-show_streams", "-of", "json", path])
        .output()
        .ok()
        .filter(|o| o.status.success());
    let Some(v) = out.and_then(|o| serde_json::from_slice::<Value>(&o.stdout).ok()) else {
        return (Vec::new(), 0.0, None);
    };
    let mut lines = Vec::new();
    let mut seconds = 0.0;
    let mut dims = None;
    let text = |x: &Value, k: &str| x.get(k).and_then(Value::as_str).map(str::to_owned);
    let num = |x: &Value, k: &str| x.get(k).and_then(Value::as_i64);
    for st in v.get("streams").and_then(Value::as_array).into_iter().flatten() {
        let codec = text(st, "codec_name").unwrap_or_default();
        match text(st, "codec_type").as_deref() {
            Some("video") if num(st, "disposition_attached_pic").is_none() => {
                let size = match (num(st, "width"), num(st, "height")) {
                    (Some(w), Some(h)) => {
                        dims.get_or_insert((w, h));
                        format!(" {w}×{h}")
                    }
                    _ => String::new(),
                };
                let fps = text(st, "avg_frame_rate")
                    .and_then(|r| r.split_once('/').map(|(a, b)| (a.parse::<f64>().unwrap_or(0.0), b.parse::<f64>().unwrap_or(1.0))))
                    .filter(|(_, b)| *b > 0.0)
                    .map(|(a, b)| a / b)
                    .filter(|f| *f > 0.0)
                    .map(|f| format!(" @ {} fps", f.round()))
                    .unwrap_or_default();
                lines.push(format!("Video: {codec}{size}{fps}"));
            }
            Some("audio") => {
                let rate = text(st, "sample_rate").map(|r| format!(" {r} Hz")).unwrap_or_default();
                let ch = num(st, "channels")
                    .map(|c| if c == 1 { " mono".to_owned() } else if c == 2 { " stereo".to_owned() } else { format!(" {c} ch") })
                    .unwrap_or_default();
                lines.push(format!("Audio: {codec}{rate}{ch}"));
            }
            Some("subtitle") => lines.push(format!("Subtitles: {codec}")),
            _ => {}
        }
    }
    if let Some(f) = v.get("format") {
        if let Some(d) = text(f, "duration").and_then(|d| d.parse::<f64>().ok()) {
            seconds = d;
            lines.push(format!("Duration: {}", fmt_duration(d)));
        }
        if let Some(b) = text(f, "bit_rate").and_then(|b| b.parse::<i64>().ok()) {
            lines.push(format!("Bitrate: {} kb/s", b / 1000));
        }
        if let Some(tags) = f.get("tags") {
            for key in ["title", "artist", "album", "date", "genre"] {
                // Tag names vary in case between containers.
                let found = tags.as_object().and_then(|m| m.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)));
                if let Some((_, val)) = found.and_then(|(k, v)| v.as_str().map(|s| (k, s))) {
                    lines.push(format!("{}: {}", key[..1].to_uppercase() + &key[1..], val));
                }
            }
        }
    }
    (lines, seconds, dims)
}

fn human_perms(mode: u32) -> String {
    let mut s = String::new();
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

/// Build the preview description for `path` as JSON (see QML Inspector for the schema).
pub fn preview(path: &str) -> Value {
    let v = build_preview(path);
    if let Some(kind) = v["kind"].as_str() {
        crate::state::FeatureUse::mark(&format!("preview:{kind}"));
    }
    v
}

fn build_preview(path: &str) -> Value {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return json!({"kind": "none", "path": path, "message": "Cannot read this item"});
    };
    let target_meta = fs::metadata(path).unwrap_or_else(|_| meta.clone());
    let name = Path::new(path).file_name().map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
    let ext = ext_of(path);
    let mut v = json!({
        "path": path,
        "name": name,
        "size": if target_meta.is_dir() { -1 } else { target_meta.len() as i64 },
        "mtime": target_meta.mtime(),
        "perms": human_perms(target_meta.permissions().mode()),
        "isLink": meta.file_type().is_symlink(),
    });
    if meta.file_type().is_symlink() {
        v["linkTarget"] = json!(fs::read_link(path).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default());
    }

    if target_meta.is_dir() {
        let mut entries = listing::list_dir(path);
        listing::sort_entries(&mut entries, "name", false);
        let total = entries.len();
        let dirs = entries.iter().filter(|e| e.is_dir).count();
        v["kind"] = json!("dir");
        v["total"] = json!(total);
        v["dirs"] = json!(dirs);
        v["entries"] = json!(
            entries.iter().take(200).map(|e| json!({"name": e.name, "isDir": e.is_dir})).collect::<Vec<_>>()
        );
        return v;
    }

    let mtime = target_meta.mtime();
    // The rarely used kinds (torrents, calendars, databases, certificates, DjVu, covers...) come from plugins.
    match crate::plugins::preview(path, &name, &ext, target_meta.len() as i64, mtime) {
        Some(crate::plugins::Preview::Text { text, note }) => {
            fill_text(&mut v, &text, "x.txt", false);
            v["note"] = json!(note);
            return v;
        }
        Some(crate::plugins::Preview::Image { path: image, note }) => {
            v["kind"] = json!("image");
            v["url"] = json!(file_url(&image));
            v["note"] = json!(note);
            return v;
        }
        None => {}
    }
    let unconvertible = |v: &mut Value, what: &str| {
        v["kind"] = json!("binary");
        v["message"] = json!(format!("{what} preview needs ImageMagick or ffmpeg"));
    };

    // Images Qt can decode itself.
    if IMAGE_EXT.contains(&ext.as_str()) {
        v["kind"] = json!("image");
        v["url"] = json!(file_url(path));
        v["info"] = json!(image_info(path));
        return v;
    }
    // Everything else image-like is converted to PNG (cached).
    if CONVERT_IMAGE_EXT.contains(&ext.as_str()) || ext == "tif" || ext == "tiff" {
        match converted_image(path, mtime) {
            Some(url) => {
                v["kind"] = json!("image");
                v["url"] = json!(url);
                v["note"] = json!("converted preview");
                v["info"] = json!(image_info(path));
            }
            None => unconvertible(&mut v, "This image format"),
        }
        return v;
    }
    if ext == "pdf" {
        v["kind"] = json!("image");
        match pdf_page_png(path, mtime) {
            Some(png) => {
                v["url"] = json!(file_url(&png));
                let info = cmd_lines("pdfinfo", &[path], 40);
                let pages = info.iter().find_map(|l| l.strip_prefix("Pages:")).map(|s| s.trim().to_owned());
                if let Some(p) = pages {
                    v["note"] = json!(format!("{p} {} · showing page 1", if p.trim() == "1" { "page" } else { "pages" }));
                }
            }
            None => {
                v["kind"] = json!("binary");
                v["message"] = json!("PDF preview needs poppler (pdftoppm)");
            }
        }
        return v;
    }
    // Word processors, spreadsheets, presentations: extracted text.
    if OFFICE_EXT.contains(&ext.as_str())
        && let Some(text) = office_text(path, &ext) {
            fill_text(&mut v, &text, "x.txt", false);
            v["note"] = json!("extracted text");
            return v;
        }
    if ARCHIVE_EXT.contains(&ext.as_str()) || COVER_EXT.contains(&ext.as_str()) || OFFICE_EXT.contains(&ext.as_str()) {
        v["kind"] = json!("archive");
        let lines = cmd_lines("bsdtar", &["-tf", path], 300);
        v["entries"] = json!(lines);
        return v;
    }
    if AUDIO_EXT.contains(&ext.as_str()) || VIDEO_EXT.contains(&ext.as_str()) {
        let video = VIDEO_EXT.contains(&ext.as_str());
        v["kind"] = json!(if video { "video" } else { "audio" });
        v["url"] = json!(file_url(path));
        let (probe, seconds, dims) = media_probe(path);
        v["info"] = json!(probe);
        if let Some((w, h)) = dims {
            v["dims"] = json!([w, h]);
        }
        if video {
            if let Some(poster) = video_poster(path, mtime, seconds) {
                v["poster"] = json!(poster);
            }
        } else {
            let accent = palette().get("accent").map_or("89b4fa".to_owned(), |c| c.trim_start_matches('#').to_owned());
            if let Some((poster, what)) = audio_art(path, mtime, &accent) {
                v["poster"] = json!(poster);
                v["posterNote"] = json!(what);
            }
        }
        return v;
    }
    if FONT_EXT.contains(&ext.as_str()) {
        v["kind"] = json!("font");
        v["url"] = json!(file_url(path));
        v["info"] = json!(font_info(path));
        return v;
    }
    match read_head(path, MAX_TEXT_BYTES) {
        Some(bytes) if !looks_binary(&bytes) => {
            let truncated = bytes.len() >= MAX_TEXT_BYTES;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            match ext.as_str() {
                "md" | "markdown" => {
                    v["kind"] = json!("markdown");
                    v["text"] = json!(text);
                    v["lines"] = json!(text.lines().count());
                    v["truncated"] = json!(truncated);
                }
                "csv" | "tsv" => {
                    let delim = if ext == "tsv" { '\t' } else { ',' };
                    fill_text(&mut v, &table_text(&text, delim, 300), "x.txt", truncated);
                    v["note"] = json!("table");
                }
                _ => {
                    let shown = if matches!(ext.as_str(), "json" | "geojson" | "ipynb") && !truncated {
                        pretty_json(&text).unwrap_or(text)
                    } else {
                        text
                    };
                    fill_text(&mut v, &shown, path, truncated);
                }
            }
        }
        Some(bytes) => {
            // Binary: a description plus the first bytes as a hex dump.
            let desc = cmd_lines("file", &["-b", path], 1).pop().unwrap_or_default();
            fill_text(&mut v, &hex_dump(&bytes[..bytes.len().min(2048)]), "x.txt", bytes.len() >= 2048);
            v["note"] = json!(format!("hex dump{}", if desc.is_empty() { String::new() } else { format!(" · {desc}") }));
            v["binary"] = json!(true);
        }
        None => {
            v["kind"] = json!("none");
            v["message"] = json!("Cannot read this file");
        }
    }
    v
}

/// Fill the text-preview fields of `v`: line count, truncation flag and highlighted HTML.
fn fill_text(v: &mut Value, text: &str, syntax_path: &str, truncated: bool) {
    // Cap the number of lines too: very long single-line files render slowly.
    let mut text = text.to_owned();
    let mut cut = false;
    let mut lines = 0;
    if let Some((idx, _)) = text.match_indices('\n').find(|_| {
        lines += 1;
        lines >= MAX_TEXT_LINES
    }) {
        text.truncate(idx);
        cut = true;
    }
    v["kind"] = json!("text");
    v["lines"] = json!(text.lines().count());
    v["truncated"] = json!(truncated || cut);
    v["html"] = json!(highlight_html(&text, syntax_path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_binary() {
        assert!(looks_binary(&[0x7f, b'E', b'L', b'F', 0, 1]));
        assert!(!looks_binary("héllo\n".as_bytes()));
        // Multibyte character cut in half at the buffer end is still text.
        assert!(!looks_binary(&"aé".as_bytes()[..2]));
    }

    #[test]
    fn highlights_rust() {
        let html = highlight_html("fn main() { let x = \"hi\"; }\n", "x.rs");
        assert!(html.contains("<span"), "{html}");
        assert!(html.contains("&nbsp;"));
    }

    #[test]
    fn plain_text_is_escaped_and_numbered() {
        let html = highlight_html("a < b\nc", "x.unknownext");
        assert!(html.contains("a&nbsp;&lt;&nbsp;b<br>"));
        assert!(html.contains("&nbsp;&nbsp;1</span>"));
        assert!(html.contains("&nbsp;&nbsp;2</span>"));
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_duration(3.0), "0:03");
        assert_eq!(fmt_duration(3725.0), "1:02:05");
    }

    #[test]
    fn perms() {
        assert_eq!(human_perms(0o755), "rwxr-xr-x");
        assert_eq!(human_perms(0o640), "rw-r-----");
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use crate::testutil::Sandbox;

    #[test]
    fn text_files_get_highlighted_html_and_line_counts() {
        let sb = Sandbox::new("preview-text");
        let f = sb.touch("a.rs", "fn main() {}\nlet x = 1;\n");
        let v = preview(&f.to_string_lossy());
        assert_eq!(v["kind"], "text");
        assert_eq!(v["lines"], 2);
        assert_eq!(v["truncated"], false);
        assert!(v["html"].as_str().unwrap().contains("<span"));
        assert_eq!(v["name"], "a.rs");
        assert_eq!(v["perms"].as_str().unwrap().len(), 9);
    }

    #[test]
    fn folders_list_entries_with_counts() {
        let sb = Sandbox::new("preview-dir");
        sb.touch("d/sub/", "");
        sb.touch("d/a.txt", "x");
        sb.touch("d/b.txt", "x");
        let v = preview(&sb.path("d"));
        assert_eq!(v["kind"], "dir");
        assert_eq!((v["total"].as_i64(), v["dirs"].as_i64()), (Some(3), Some(1)));
        assert_eq!(v["entries"][0]["name"], "sub", "folders sort first");
    }

    #[test]
    fn binary_images_and_unreadable_items() {
        let sb = Sandbox::new("preview-misc");
        let bin = sb.touch("blob.bin", "");
        fs::write(&bin, [0u8, 159, 146, 150, 0, 1]).unwrap();
        let b = preview(&bin.to_string_lossy());
        assert_eq!(b["kind"], "text");
        assert_eq!(b["binary"], true);
        assert!(b["note"].as_str().unwrap().starts_with("hex dump"));
        assert!(b["html"].as_str().unwrap().contains("00000000"));
        let png = sb.touch("pic.PNG", "not really a png");
        let v = preview(&png.to_string_lossy());
        assert_eq!(v["kind"], "image");
        assert!(v["url"].as_str().unwrap().starts_with("file://"));
        assert_eq!(preview("/definitely/not/here")["kind"], "none");
    }

    #[test]
    fn long_text_is_truncated() {
        let sb = Sandbox::new("preview-long");
        let body: String = (0..MAX_TEXT_LINES + 50).map(|i| format!("line {i}\n")).collect();
        let v = preview(&sb.touch("long.txt", &body).to_string_lossy());
        assert_eq!(v["truncated"], true);
        assert!(v["lines"].as_i64().unwrap() <= MAX_TEXT_LINES as i64);
    }

    #[test]
    fn symlinks_report_their_target() {
        let sb = Sandbox::new("preview-link");
        sb.touch("real.txt", "x");
        std::os::unix::fs::symlink(sb.path("real.txt"), sb.path("link.txt")).unwrap();
        let v = preview(&sb.path("link.txt"));
        assert_eq!(v["isLink"], true);
        assert_eq!(v["linkTarget"], sb.path("real.txt"));
    }
}

#[cfg(test)]
mod format_tests {
    use super::*;
    use crate::testutil::Sandbox;

    fn have(tool: &str) -> bool {
        Command::new(tool).arg("--version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok()
    }

    #[test]
    fn csv_becomes_an_aligned_table() {
        let sb = Sandbox::new("pv-csv");
        let v = preview(&sb.touch("t.csv", "name,qty\napple,3\n").to_string_lossy());
        assert_eq!(v["kind"], "text");
        assert_eq!(v["note"], "table");
        assert!(v["html"].as_str().unwrap().contains("apple"));
    }

    #[test]
    fn markdown_keeps_its_source_for_rendering() {
        let sb = Sandbox::new("pv-md");
        let v = preview(&sb.touch("a.md", "# Hi\n\n- x\n").to_string_lossy());
        assert_eq!(v["kind"], "markdown");
        assert!(v["text"].as_str().unwrap().starts_with("# Hi"));
    }

    #[test]
    fn minified_json_is_pretty_printed() {
        let sb = Sandbox::new("pv-json");
        let v = preview(&sb.touch("a.json", "{\"a\":[1,2]}").to_string_lossy());
        assert_eq!(v["kind"], "text");
        assert!(v["lines"].as_i64().unwrap() > 3);
    }

    #[test]
    fn office_documents_show_their_text() {
        if !have("bsdtar") {
            return;
        }
        let sb = Sandbox::new("pv-docx");
        sb.touch("src/word/document.xml", "<w:p><w:r><w:t>Hello docx</w:t></w:r></w:p>");
        let out = sb.path("a.docx");
        let made = Command::new("bsdtar")
            .args(["--format", "zip", "-cf", &out, "-C", &sb.path("src"), "word"])
            .status()
            .is_ok_and(|s| s.success());
        if !made {
            return;
        }
        let v = preview(&out);
        assert_eq!(v["kind"], "text");
        assert_eq!(v["note"], "extracted text");
        assert!(v["html"].as_str().unwrap().contains("Hello"));
    }

    #[test]
    fn unknown_binaries_get_a_hex_dump() {
        let sb = Sandbox::new("pv-hex");
        let f = sb.touch("x.dat", "");
        fs::write(&f, [0u8, 1, 2, 3, 255, 254]).unwrap();
        let v = preview(&f.to_string_lossy());
        assert_eq!(v["binary"], true);
        assert!(v["html"].as_str().unwrap().contains("00000000"));
    }

    #[test]
    fn image_urls_are_percent_encoded() {
        let sb = Sandbox::new("pv-url");
        let v = preview(&sb.touch("my pic #1.png", "x").to_string_lossy());
        assert_eq!(v["kind"], "image");
        let url = v["url"].as_str().unwrap();
        assert!(url.contains("my%20pic%20%231.png"), "{url}");
    }

    #[test]
    fn video_previews_carry_a_poster_when_ffmpeg_is_present() {
        if !have("ffmpeg") {
            return;
        }
        let sb = Sandbox::new("pv-video");
        let clip = sb.path("c.mp4");
        let made = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=160x90:rate=10", "-pix_fmt", "yuv420p", &clip])
            .status()
            .is_ok_and(|s| s.success());
        if !made {
            return;
        }
        let v = preview(&clip);
        assert_eq!(v["kind"], "video");
        assert!(v["poster"].as_str().unwrap().ends_with(".png"));
        assert!(v["info"].as_array().unwrap().iter().any(|l| l.as_str().unwrap().starts_with("Video: h264")));
    }
}
