//! Previews beyond plain text: video posters, audio art and waveforms, converted images, e-book
//! covers, office documents, CSV tables, SQLite and certificate summaries, fonts and hex dumps.
//! Helpers that run external tools return `None` when the tool is missing, and callers fall back.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::ops::percent_encode_path;
use crate::state::cache_dir;

pub fn file_url(path: &str) -> String {
    format!("file://{}", percent_encode_path(path))
}

fn run_ok(cmd: &mut Command) -> bool {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn output(cmd: &mut Command) -> Option<Vec<u8>> {
    let out = cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// A cache file named after `(path, mtime, tag)`. Returns it if present, else builds it with `build`.
fn cached(path: &str, mtime: i64, tag: &str, ext: &str, build: impl FnOnce(&Path) -> bool) -> Option<PathBuf> {
    let mut h = DefaultHasher::new();
    (path, mtime, tag).hash(&mut h);
    let dir = cache_dir().join("preview");
    fs::create_dir_all(&dir).ok()?;
    let file = dir.join(format!("{:x}.{ext}", h.finish()));
    if file.exists() && fs::metadata(&file).is_ok_and(|m| m.len() > 0) {
        return Some(file);
    }
    let tmp = file.with_extension(format!("tmp.{ext}"));
    if build(&tmp) && fs::rename(&tmp, &file).is_ok() {
        Some(file)
    } else {
        let _ = fs::remove_file(&tmp);
        None
    }
}

// ---- video and audio -----------------------------------------------------------------------

/// A poster frame (taken ~10% in) of a video, as a file URL.
pub fn video_poster(path: &str, mtime: i64, duration: f64) -> Option<String> {
    let at = if duration > 3.0 { duration * 0.1 } else { 0.0 };
    cached(path, mtime, "poster", "png", |out| {
        run_ok(
            Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-ss", &format!("{at:.2}"), "-i", path, "-frames:v", "1", "-vf", "scale=960:-2"])
                .arg(out),
        )
    })
    .map(|p| file_url(&p.to_string_lossy()))
}

/// Embedded cover art of an audio file, else a waveform image in `accent` (hex without '#').
pub fn audio_art(path: &str, mtime: i64, accent: &str) -> Option<(String, &'static str)> {
    if let Some(p) = cached(path, mtime, "cover", "png", |out| {
        run_ok(Command::new("ffmpeg").args(["-v", "error", "-y", "-i", path, "-an", "-frames:v", "1", "-vf", "scale=600:-2"]).arg(out))
    }) {
        return Some((file_url(&p.to_string_lossy()), "cover art"));
    }
    let color = format!("0x{accent}");
    cached(path, mtime, &format!("wave{accent}"), "png", |out| {
        run_ok(
            Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-i", path, "-filter_complex"])
                .arg(format!("showwavespic=s=900x180:colors={color}"))
                .args(["-frames:v", "1"])
                .arg(out),
        )
    })
    .map(|p| (file_url(&p.to_string_lossy()), "waveform"))
}

// ---- images --------------------------------------------------------------------------------

/// Formats Qt cannot decode itself; converted to PNG with ImageMagick or ffmpeg.
pub const CONVERT_IMAGE_EXT: &[&str] = &[
    "heic", "heif", "avif", "jxl", "psd", "xcf", "exr", "hdr", "tga", "dds", "jp2", "pnm", "ppm", "pgm", "pbm", "ai", "eps",
    "cr2", "cr3", "nef", "arw", "dng", "orf", "rw2", "raf", "pef", "srw", "kra", "ora", "xpm", "pcx", "sgi",
];

pub fn converted_image(path: &str, mtime: i64) -> Option<String> {
    cached(path, mtime, "convert", "png", |out| {
        let src = format!("{path}[0]");
        run_ok(Command::new("magick").arg(&src).args(["-auto-orient", "-resize", "1400x1400>"]).arg(out))
            || run_ok(Command::new("ffmpeg").args(["-v", "error", "-y", "-i", path, "-frames:v", "1"]).arg(out))
    })
    .map(|p| file_url(&p.to_string_lossy()))
}

/// `WxH`, camera and date of an image (ImageMagick `identify`), as short info lines.
pub fn image_info(path: &str) -> Vec<String> {
    let fmt = "%w×%h|%[EXIF:Make] %[EXIF:Model]|%[EXIF:DateTimeOriginal]";
    let Some(bytes) = output(Command::new("magick").args(["identify", "-format", fmt]).arg(format!("{path}[0]"))) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut parts = text.split('|').map(str::trim);
    let mut out = Vec::new();
    if let Some(dims) = parts.next().filter(|d| !d.is_empty()) {
        out.push(dims.to_owned());
    }
    for extra in parts {
        // Unset EXIF fields come back as literal "unknown" or empty.
        if !extra.is_empty() && !extra.contains("unknown") && extra != " " {
            out.push(extra.to_owned());
        }
    }
    out
}

// ---- archives with covers and office documents ----------------------------------------------

fn archive_members(path: &str) -> Vec<String> {
    output(Command::new("bsdtar").args(["-tf", path]))
        .map(|b| String::from_utf8_lossy(&b).lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

fn read_member(path: &str, member: &str) -> Option<Vec<u8>> {
    output(Command::new("bsdtar").args(["-xOf", path, member]))
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Text of an OOXML/ODF XML part: closing paragraph-like tags become newlines, other tags vanish.
pub fn xml_to_text(xml: &str, break_tags: &[&str]) -> String {
    let mut out = String::with_capacity(xml.len() / 3);
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        out.push_str(&decode_entities(&rest[..open]));
        let Some(close) = rest[open..].find('>') else { break };
        let tag = &rest[open + 1..open + close];
        if break_tags.iter().any(|t| tag == format!("/{t}") || tag.starts_with(&format!("{t} /")) || tag == format!("{t}/")) {
            out.push('\n');
        } else if tag == "w:tab/" || tag == "w:tab /" {
            out.push('\t');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(&decode_entities(rest));
    // Collapse runs of blank lines.
    let mut cleaned = String::new();
    let mut blank = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        cleaned.push_str(line.trim_end());
        cleaned.push('\n');
    }
    cleaned
}

/// Plain text of a word-processor, spreadsheet or presentation file.
pub fn office_text(path: &str, ext: &str) -> Option<String> {
    let text = match ext {
        "docx" | "docm" | "dotx" => xml_to_text(&String::from_utf8_lossy(&read_member(path, "word/document.xml")?), &["w:p"]),
        "xlsx" | "xlsm" => {
            let strings = read_member(path, "xl/sharedStrings.xml")?;
            xml_to_text(&String::from_utf8_lossy(&strings), &["si"])
        }
        "pptx" | "pptm" => {
            let mut slides: Vec<String> = archive_members(path)
                .into_iter()
                .filter(|m| m.starts_with("ppt/slides/slide") && m.ends_with(".xml"))
                .collect();
            slides.sort_by_key(|m| m.trim_start_matches("ppt/slides/slide").trim_end_matches(".xml").parse::<u32>().unwrap_or(0));
            let mut all = String::new();
            for (i, m) in slides.iter().take(30).enumerate() {
                if let Some(xml) = read_member(path, m) {
                    all.push_str(&format!("— slide {} —\n", i + 1));
                    all.push_str(&xml_to_text(&String::from_utf8_lossy(&xml), &["a:p"]));
                }
            }
            all
        }
        "odt" | "ods" | "odp" | "fodt" => {
            xml_to_text(&String::from_utf8_lossy(&read_member(path, "content.xml")?), &["text:p", "text:h", "table:table-row"])
        }
        _ => return None,
    };
    (!text.trim().is_empty()).then_some(text)
}

// ---- torrents, RTF, executables, DjVu --------------------------------------------------------

// ---- tables, hex, json, sqlite, certificates, fonts -----------------------------------------

/// Split one CSV/TSV line, honouring double quotes.
pub fn split_row(line: &str, delim: char) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            c if c == delim && !quoted => cells.push(std::mem::take(&mut cur)),
            c => cur.push(c),
        }
    }
    cells.push(cur);
    cells
}

/// Column-aligned plain text for a delimited file (first `max_rows` rows); the first row is the header.
pub fn table_text(text: &str, delim: char, max_rows: usize) -> String {
    let rows: Vec<Vec<String>> = text.lines().take(max_rows).map(|l| split_row(l, delim)).collect();
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; cols];
    for r in &rows {
        for (i, c) in r.iter().enumerate() {
            widths[i] = widths[i].max(c.chars().count().min(28));
        }
    }
    let mut out = String::new();
    for (n, r) in rows.iter().enumerate() {
        let line: Vec<String> = (0..cols)
            .map(|i| {
                let c = r.get(i).map_or("", String::as_str);
                let clipped: String = if c.chars().count() > 28 { c.chars().take(27).chain("…".chars()).collect() } else { c.to_owned() };
                format!("{clipped:<w$}", w = widths[i])
            })
            .collect();
        out.push_str(line.join("  ").trim_end());
        out.push('\n');
        if n == 0 {
            out.push_str(&widths.iter().map(|w| "─".repeat(*w)).collect::<Vec<_>>().join("──"));
            out.push('\n');
        }
    }
    out
}

/// Classic hex dump: offset, 16 bytes in hex, ASCII.
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk.iter().map(|&b| if (0x20..0x7f).contains(&b) { b as char } else { '.' }).collect();
        out.push_str(&format!("{:08x}  {:<47}  {ascii}\n", i * 16, hex.join(" ")));
    }
    out
}

/// Pretty-print minified JSON; `None` when it is not valid JSON or already multi-line.
pub fn pretty_json(text: &str) -> Option<String> {
    if text.trim().lines().count() > 3 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    serde_json::to_string_pretty(&v).ok()
}

/// Family, style and file info of a font via `fc-scan`.
pub fn font_info(path: &str) -> Vec<String> {
    output(Command::new("fc-scan").args(["--format", "%{family[0]}\n%{style[0]}\n%{fullname[0]}\n%{fontformat}\n", path]))
        .map(|b| String::from_utf8_lossy(&b).lines().filter(|l| !l.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_becomes_paragraph_text() {
        let xml = "<w:p><w:r><w:t>Hello &amp; welcome</w:t></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p>";
        assert_eq!(xml_to_text(xml, &["w:p"]), "Hello & welcome\nSecond\n");
        // Blank runs collapse.
        assert_eq!(xml_to_text("<a:p></a:p><a:p></a:p><a:p>x</a:p>", &["a:p"]), "\nx\n");
    }

    #[test]
    fn csv_rows_split_with_quotes() {
        assert_eq!(split_row(r#"a,"b,c","say ""hi""",d"#, ','), ["a", "b,c", "say \"hi\"", "d"]);
        assert_eq!(split_row("x\ty\t", '\t'), ["x", "y", ""]);
    }

    #[test]
    fn tables_are_aligned_with_a_header_rule() {
        let t = table_text("name,qty\napple,3\nkiwi,12\n", ',', 100);
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[0], "name   qty");
        assert!(lines[1].starts_with('─'));
        assert_eq!(lines[2], "apple  3");
        assert_eq!(lines[3], "kiwi   12");
    }

    #[test]
    fn hex_dump_layout() {
        let d = hex_dump(b"Hello, world!\x00\x01\x02ABC");
        let first = d.lines().next().unwrap();
        assert!(first.starts_with("00000000  48 65 6c 6c 6f 2c 20 77"));
        assert!(first.ends_with("Hello, world!..."));
        assert!(d.lines().nth(1).unwrap().starts_with("00000010  "));
    }

    #[test]
    fn json_pretty_only_for_compact_input() {
        assert!(pretty_json("{\"a\":[1,2],\"b\":{\"c\":null}}").unwrap().contains("\n  \"a\": ["));
        assert!(pretty_json("{\n\"a\":1,\n\"b\":2,\n\"c\":3\n}").is_none());
        assert!(pretty_json("not json").is_none());
    }

    #[test]
    fn urls_are_percent_encoded() {
        assert_eq!(file_url("/a b/c#d.png"), "file:///a%20b/c%23d.png");
    }

}

