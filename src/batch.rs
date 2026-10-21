//! Batch rename: template tokens, find/replace (plain or regex), case conversion, live preview.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Opts {
    /// Tokens: `{name}` stem, `{ext}` extension with its dot, `{old}` full old name,
    /// `{n}` / `{n:3}` counter (zero padded), `{date}` modified date, `{created:%Y-%m-%d}` /
    /// `{modified:%H%M}` with a strftime format, `{parent}` folder name,
    /// `{id}` short random id, `{uuid}` full random UUID.
    pub template: String,
    pub find: String,
    pub replace: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// keep | lower | upper | title | snake | kebab
    pub case_mode: String,
    pub start: i64,
    pub step: i64,
    /// Order in which files get their counter: `name` or `date`.
    pub sort: String,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            template: "{name}{ext}".into(),
            find: String::new(),
            replace: String::new(),
            regex: false,
            case_sensitive: true,
            case_mode: "keep".into(),
            start: 1,
            step: 1,
            sort: "name".into(),
        }
    }
}

#[derive(Serialize, Debug, PartialEq)]
pub struct Row {
    pub from: String,
    pub to: String,
    pub name: String,
    #[serde(rename = "newName")]
    pub new_name: String,
    pub changed: bool,
    pub conflict: bool,
}

fn split_name(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn local_date(mtime: i64) -> String {
    // SAFETY: localtime_r only writes into the zeroed `tm` we pass.
    unsafe {
        let t: libc::time_t = mtime as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return String::new();
        }
        format!("{:04}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday)
    }
}

/// strftime-style formatting in local time (`%Y-%m-%d`, `%H%M`, ...).
fn format_time(t: i64, fmt: &str) -> String {
    let Ok(cfmt) = std::ffi::CString::new(fmt) else { return String::new() };
    // SAFETY: localtime_r/strftime only write into the buffers we pass; the length is checked.
    unsafe {
        let secs: libc::time_t = t as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&secs, &mut tm).is_null() {
            return String::new();
        }
        let mut buf = [0u8; 128];
        let n = libc::strftime(buf.as_mut_ptr().cast(), buf.len(), cfmt.as_ptr(), &tm);
        String::from_utf8_lossy(&buf[..n]).into_owned()
    }
}

fn unique_id(full: bool) -> String {
    let uuid = std::fs::read_to_string("/proc/sys/kernel/random/uuid").unwrap_or_default();
    let uuid = uuid.trim();
    if uuid.is_empty() {
        // Fallback: clock nanoseconds are unique enough between rows.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        return format!("{n:x}");
    }
    if full { uuid.to_owned() } else { uuid.replace('-', "")[..8].to_owned() }
}

/// Expand template tokens for one file.
fn expand(template: &str, old_name: &str, parent: &str, mtime: i64, created: i64, counter: i64) -> String {
    let (stem, ext) = split_name(old_name);
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let token = &after[..close];
        let (key, arg) = token.split_once(':').map_or((token, ""), |(k, a)| (k, a));
        match key {
            "name" => out.push_str(stem),
            "ext" => out.push_str(ext),
            "old" => out.push_str(old_name),
            "parent" => out.push_str(parent),
            "date" => out.push_str(&local_date(mtime)),
            "created" => out.push_str(&format_time(created, if arg.is_empty() { "%Y-%m-%d" } else { arg })),
            "modified" => out.push_str(&format_time(mtime, if arg.is_empty() { "%Y-%m-%d" } else { arg })),
            "id" => out.push_str(&unique_id(false)),
            "uuid" => out.push_str(&unique_id(true)),
            "n" => {
                let width: usize = arg.parse().unwrap_or(0);
                out.push_str(&format!("{counter:0width$}"));
            }
            _ => {
                out.push('{');
                out.push_str(token);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn words(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut prev_lower = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if c.is_uppercase() && prev_lower && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            prev_lower = c.is_lowercase() || c.is_ascii_digit();
            cur.push(c);
        } else {
            prev_lower = false;
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn title_word(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

pub fn convert_case(stem: &str, mode: &str) -> String {
    match mode {
        "lower" => stem.to_lowercase(),
        "upper" => stem.to_uppercase(),
        "title" => stem
            .split(' ')
            .map(title_word)
            .collect::<Vec<_>>()
            .join(" "),
        "snake" => words(stem).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_"),
        "kebab" => words(stem).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("-"),
        _ => stem.to_owned(),
    }
}

enum Finder {
    None,
    Plain { needle: String, sensitive: bool },
    Pattern(Regex),
}

fn finder(opts: &Opts) -> Result<Finder, String> {
    if opts.find.is_empty() {
        return Ok(Finder::None);
    }
    if opts.regex {
        RegexBuilder::new(&opts.find)
            .case_insensitive(!opts.case_sensitive)
            .build()
            .map(Finder::Pattern)
            .map_err(|e| e.to_string().lines().last().unwrap_or("invalid regex").trim().to_owned())
    } else {
        Ok(Finder::Plain { needle: opts.find.clone(), sensitive: opts.case_sensitive })
    }
}

fn apply_find(f: &Finder, name: &str, replace: &str) -> String {
    match f {
        Finder::None => name.to_owned(),
        Finder::Pattern(re) => re.replace_all(name, replace).into_owned(),
        Finder::Plain { needle, sensitive: true } => name.replace(needle.as_str(), replace),
        Finder::Plain { needle, sensitive: false } => {
            // Case-insensitive literal replace via an escaped regex.
            match RegexBuilder::new(&regex::escape(needle)).case_insensitive(true).build() {
                Ok(re) => re.replace_all(name, regex::NoExpand(replace)).into_owned(),
                Err(_) => name.to_owned(),
            }
        }
    }
}

/// Compute the new names. `Err` carries a message for an invalid regex.
pub fn preview(paths: &[String], opts: &Opts) -> Result<Vec<Row>, String> {
    let finder = finder(opts)?;
    let mut rows: Vec<Row> = Vec::with_capacity(paths.len());
    // Counters follow the chosen order; the preview lists files in that order.
    let mut ordered: Vec<&String> = paths.iter().collect();
    if opts.sort == "date" {
        ordered.sort_by_key(|p| fs::symlink_metadata(p.as_str()).map_or(0, |m| m.mtime()));
    }
    for (i, from) in ordered.into_iter().enumerate() {
        let p = Path::new(from);
        let name = p.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let parent_dir = p.parent().unwrap_or(Path::new("/"));
        let parent_name = parent_dir.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let meta = fs::symlink_metadata(from).ok();
        let mtime = meta.as_ref().map_or(0, |m| m.mtime());
        let created = meta
            .as_ref()
            .and_then(|m| m.created().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(mtime, |d| d.as_secs() as i64);
        let counter = opts.start + opts.step * i as i64;

        let expanded = expand(&opts.template, &name, &parent_name, mtime, created, counter);
        let replaced = apply_find(&finder, &expanded, &opts.replace);
        let (stem, ext) = split_name(&replaced);
        let new_name = format!("{}{}", convert_case(stem, &opts.case_mode), ext);
        let to = parent_dir.join(&new_name).to_string_lossy().into_owned();
        rows.push(Row { changed: new_name != name, from: from.clone(), to, name, new_name, conflict: false });
    }

    // Conflicts: invalid names, duplicates among the targets, or clobbering an untouched file.
    let moving: HashSet<&str> = rows.iter().filter(|r| r.changed).map(|r| r.from.as_str()).collect();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for r in &rows {
        *seen.entry(r.to.clone()).or_default() += 1;
    }
    let flags: Vec<bool> = rows
        .iter()
        .map(|r| {
            if !r.changed {
                return false;
            }
            r.new_name.is_empty()
                || r.new_name == "."
                || r.new_name == ".."
                || r.new_name.contains('/')
                || seen.get(&r.to).copied().unwrap_or(0) > 1
                || (fs::symlink_metadata(&r.to).is_ok() && !moving.contains(r.to.as_str()))
        })
        .collect();
    for (r, f) in rows.iter_mut().zip(flags) {
        r.conflict = f;
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(paths: &[&str], opts: &Opts) -> Vec<String> {
        let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        preview(&paths, opts).unwrap().into_iter().map(|r| r.new_name).collect()
    }

    #[test]
    fn counter_and_template() {
        let opts = Opts { template: "photo_{n:3}{ext}".into(), start: 7, ..Default::default() };
        assert_eq!(names(&["/x/a.jpg", "/x/b.png"], &opts), ["photo_007.jpg", "photo_008.png"]);
    }

    #[test]
    fn find_replace_plain_and_regex() {
        let plain = Opts { find: "old".into(), replace: "new".into(), ..Default::default() };
        assert_eq!(names(&["/x/old_old.txt"], &plain), ["new_new.txt"]);
        let re = Opts { find: r"(\d+)".into(), replace: "<$1>".into(), regex: true, ..Default::default() };
        assert_eq!(names(&["/x/img12.png"], &re), ["img<12>.png"]);
        let ci = Opts { find: "OLD".into(), replace: "n".into(), case_sensitive: false, ..Default::default() };
        assert_eq!(names(&["/x/old.txt"], &ci), ["n.txt"]);
    }

    #[test]
    fn bad_regex_is_an_error() {
        let opts = Opts { find: "(".into(), regex: true, ..Default::default() };
        assert!(preview(&["/x/a".to_owned()], &opts).is_err());
    }

    #[test]
    fn case_conversion_keeps_extension() {
        let snake = Opts { case_mode: "snake".into(), ..Default::default() };
        assert_eq!(names(&["/x/My Great-File.TXT"], &snake), ["my_great_file.TXT"]);
        let title = Opts { case_mode: "title".into(), ..Default::default() };
        assert_eq!(names(&["/x/hello world.md"], &title), ["Hello World.md"]);
        let camel = Opts { case_mode: "kebab".into(), ..Default::default() };
        assert_eq!(names(&["/x/helloWorldFoo.md"], &camel), ["hello-world-foo.md"]);
    }

    #[test]
    fn duplicates_are_flagged() {
        let opts = Opts { template: "same{ext}".into(), ..Default::default() };
        let rows = preview(&["/x/a.txt".to_owned(), "/x/b.txt".to_owned()], &opts).unwrap();
        assert!(rows.iter().all(|r| r.conflict));
    }

    #[test]
    fn ids_are_unique_per_file() {
        let opts = Opts { template: "{id}{ext}".into(), ..Default::default() };
        let n = names(&["/x/a.txt", "/x/b.txt"], &opts);
        assert_ne!(n[0], n[1]);
        assert_eq!(n[0].len(), 8 + 4);
    }

    #[test]
    fn unknown_tokens_stay_literal() {
        assert_eq!(expand("{nope}-{name}", "a.b", "p", 0, 0, 1), "{nope}-a");
        assert_eq!(expand("{name", "a.b", "p", 0, 0, 1), "{name");
        assert_eq!(expand("{created:%Y}", "a.b", "p", 86_400 * 366, 86_400 * 366, 1).len(), 4);
    }
}
