//! Fuzzy matching and the small query language shared by folder filter and deep search.
//!
//! A query is whitespace-separated terms that must all match (fzf-style). Terms of the form
//! `ext:rs`, `type:d|f`, `newer:7d`, `older:2w`, `size>10M` or `size<1k` are filters instead.

use std::time::{SystemTime, UNIX_EPOCH};

const SCORE_MATCH: i32 = 16;
const BONUS_BOUNDARY: i32 = 10;
const BONUS_CONSECUTIVE: i32 = 8;
const BONUS_FIRST: i32 = 15;

fn is_separator(c: char) -> bool {
    matches!(c, '/' | '.' | '_' | '-' | ' ' | ':')
}

fn fold(c: char) -> char {
    // Single-char lowercase is enough for matching; avoids allocating.
    if c.is_ascii() {
        c.to_ascii_lowercase()
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Cheap allocation-free check that `needle` is a subsequence of `text`.
fn is_subsequence(needle: &[char], case_sensitive: bool, text: &str) -> bool {
    let mut it = needle.iter();
    let Some(mut want) = it.next() else {
        return true;
    };
    for c in text.chars() {
        let c = if case_sensitive { c } else { fold(c) };
        if c == *want {
            match it.next() {
                Some(next) => want = next,
                None => return true,
            }
        }
    }
    false
}

/// Score `text` against `needle` (already case-prepared by the caller via `smart_case`).
/// Returns `None` when `needle` is not a subsequence of `text`. Allocation-free.
pub fn score(needle: &[char], case_sensitive: bool, text: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }
    if !is_subsequence(needle, case_sensitive, text) {
        return None;
    }
    let eq = |a: char, b: char| if case_sensitive { a == b } else { fold(a) == b };

    // Forward pass: byte offset just past the earliest end of a match.
    let mut ni = 0;
    let mut end = 0;
    for (i, c) in text.char_indices() {
        if eq(c, needle[ni]) {
            ni += 1;
            if ni == needle.len() {
                end = i + c.len_utf8();
                break;
            }
        }
    }

    // Backward pass from `end`: tighten the window to the latest possible start.
    let mut ni = needle.len();
    let mut start = 0;
    for (i, c) in text[..end].char_indices().rev() {
        if eq(c, needle[ni - 1]) {
            ni -= 1;
            if ni == 0 {
                start = i;
                break;
            }
        }
    }

    // Score the window left to right.
    let mut total = 0;
    let mut ni = 0;
    let mut prev = text[..start].chars().next_back();
    let mut prev_matched = false;
    let mut run = 0;
    for c in text[start..end].chars() {
        if ni < needle.len() && eq(c, needle[ni]) {
            total += SCORE_MATCH;
            let boundary = match prev {
                None => true,
                Some(p) => is_separator(p) || (p.is_lowercase() && c.is_uppercase()),
            };
            if boundary {
                total += BONUS_BOUNDARY;
            }
            if prev.is_none() {
                total += BONUS_FIRST;
            }
            if prev_matched {
                run += 1;
                total += BONUS_CONSECUTIVE * run.min(3);
            } else {
                run = 0;
            }
            prev_matched = true;
            ni += 1;
        } else {
            prev_matched = false;
            run = 0;
            total -= 1;
        }
        prev = Some(c);
    }
    // Prefer shorter candidates.
    Some(total - (text.len() as i32) / 8)
}

pub use crate::listing::natural_cmp as natural_cmp_pub;

#[derive(Clone, Debug, Default)]
pub struct Query {
    terms: Vec<(Vec<char>, bool)>,
    /// The plain search words, for zoxide-style ordered matching.
    words: Vec<String>,
    /// `re:<pattern>` queries match names with a regular expression instead.
    regex: Option<regex::Regex>,
    bad_regex: bool,
    /// Accepted extensions (lower-case, no dot; may be compound like `tar.gz`).
    pub ext: Vec<String>,
    pub only_dirs: Option<bool>,
    /// Keep entries modified after / before this unix time.
    pub newer: Option<i64>,
    pub older: Option<i64>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
}

/// Whether `name` ends in one of `exts` (`rs` matches `main.rs`, `tar.gz` matches `a.tar.gz`).
pub fn matches_ext(exts: &[String], name: &str) -> bool {
    let lower = name.to_lowercase();
    exts.iter()
        .any(|e| lower.len() > e.len() + 1 && lower.ends_with(&format!(".{e}")))
}

fn parse_duration(s: &str) -> Option<i64> {
    let (num, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit())?);
    let n: i64 = num.parse().ok()?;
    let secs = match unit {
        "s" => 1,
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        "w" => 604_800,
        "y" => 31_536_000,
        _ => return None,
    };
    Some(n * secs)
}

fn parse_size(s: &str) -> Option<i64> {
    let s = s.to_ascii_lowercase();
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n: i64 = s[..split].parse().ok()?;
    let mult = match &s[split..] {
        "" | "b" => 1,
        "k" | "kb" => 1 << 10,
        "m" | "mb" => 1 << 20,
        "g" | "gb" => 1 << 30,
        _ => return None,
    };
    Some(n * mult)
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Query {
    pub fn parse(input: &str) -> Query {
        let mut q = Query::default();
        if let Some(pattern) = input.trim_start().strip_prefix("re:") {
            // Smart case, like the plain terms. An unfinished pattern matches nothing.
            let insensitive = !pattern.chars().any(char::is_uppercase);
            match regex::RegexBuilder::new(pattern)
                .case_insensitive(insensitive)
                .build()
            {
                Ok(re) if !pattern.is_empty() => q.regex = Some(re),
                Ok(_) => {}
                Err(_) => q.bad_regex = true,
            }
            return q;
        }
        for token in input.split_whitespace() {
            if let Some(v) = token.strip_prefix("ext:") {
                q.ext = v
                    .split(',')
                    .map(|e| e.trim_start_matches('.').to_lowercase())
                    .filter(|e| !e.is_empty())
                    .collect();
            } else if let Some(v) = token.strip_prefix("type:") {
                q.only_dirs = match v {
                    "d" | "dir" => Some(true),
                    "f" | "file" => Some(false),
                    _ => None,
                };
            } else if let Some(d) = token.strip_prefix("newer:").and_then(parse_duration) {
                q.newer = Some(now() - d);
            } else if let Some(d) = token.strip_prefix("older:").and_then(parse_duration) {
                q.older = Some(now() - d);
            } else if let Some(n) = token.strip_prefix("size>").and_then(parse_size) {
                q.min_size = Some(n);
            } else if let Some(n) = token.strip_prefix("size<").and_then(parse_size) {
                q.max_size = Some(n);
            } else {
                // Smart case: any uppercase makes the term case-sensitive.
                let sensitive = token.chars().any(char::is_uppercase);
                let chars = if sensitive {
                    token.chars().collect()
                } else {
                    token.chars().map(fold).collect()
                };
                q.terms.push((chars, sensitive));
                q.words.push(token.to_owned());
            }
        }
        q
    }

    pub fn has_terms(&self) -> bool {
        !self.terms.is_empty() || self.regex.is_some() || self.bad_regex
    }

    /// Whether the query filters on size or date, which needs the file's metadata.
    pub fn needs_meta(&self) -> bool {
        self.newer.is_some()
            || self.older.is_some()
            || self.min_size.is_some()
            || self.max_size.is_some()
    }

    /// Check the non-fuzzy filters. Unknown values (`size < 0`) never fail a filter.
    pub fn accepts(&self, name: &str, is_dir: bool, size: i64, mtime: i64) -> bool {
        if let Some(d) = self.only_dirs
            && d != is_dir
        {
            return false;
        }
        if !self.ext.is_empty() && !matches_ext(&self.ext, name) {
            return false;
        }
        if mtime > 0
            && (self.newer.is_some_and(|t| mtime < t) || self.older.is_some_and(|t| mtime > t))
        {
            return false;
        }
        if size >= 0
            && (self.min_size.is_some_and(|m| size < m) || self.max_size.is_some_and(|m| size > m))
        {
            return false;
        }
        true
    }

    /// Total fuzzy score of all terms against `name`, falling back to `path` per term.
    /// Name matches rank above matches that only hit the directory part.
    pub fn score(&self, name: &str, path: &str) -> Option<i32> {
        if self.bad_regex {
            return None;
        }
        if let Some(re) = &self.regex {
            if let Some(m) = re.find(name) {
                return Some(1000 - m.start() as i32);
            }
            return re.find(path).map(|m| 500 - m.start() as i32);
        }
        let mut total = 0;
        for (needle, sensitive) in &self.terms {
            let s = match score(needle, *sensitive, name) {
                Some(s) => s + 40,
                None => score(needle, *sensitive, path)?,
            };
            total += s;
        }
        // zoxide-style: all words as in-order substrings of the name beats a scattered fuzzy match.
        if (self.words.len() > 1 || self.words.first().is_some_and(|w| w.len() > 1))
            && let Some(ordered) = ordered_terms_score(&self.words.join(" "), name)
        {
            total += 100 + ordered;
        }
        Some(total)
    }
}

/// zoxide-style match: every whitespace-separated term must appear, in order, as a substring
/// (smart case). Terms starting at a word boundary and early matches score higher.
pub fn ordered_terms_score(query: &str, text: &str) -> Option<i32> {
    let terms: Vec<&str> = query.split_whitespace().collect();
    if terms.is_empty() {
        return Some(0);
    }
    let lower = text.to_lowercase();
    let mut from = 0;
    let mut total = 0;
    for term in terms {
        let sensitive = term.chars().any(char::is_uppercase);
        let (hay, needle) = if sensitive {
            (text, term.to_owned())
        } else {
            (lower.as_str(), term.to_lowercase())
        };
        // Byte offsets can differ between `text` and `lower` for exotic Unicode; stay in range.
        let start = from.min(hay.len());
        if !hay.is_char_boundary(start) {
            return None;
        }
        let pos = start + hay[start..].find(&needle)?;
        let boundary = pos == 0
            || !hay[..pos]
                .chars()
                .next_back()
                .is_some_and(char::is_alphanumeric);
        total += 20 + if boundary { 15 } else { 0 } - (pos as i32).min(30);
        from = pos + needle.len();
    }
    Some(total - text.len() as i32 / 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(q: &str, t: &str) -> Option<i32> {
        Query::parse(q).score(t, t)
    }

    #[test]
    fn subsequence_required() {
        assert!(s("mn", "main.rs").is_some());
        assert!(s("xyz", "main.rs").is_none());
    }

    #[test]
    fn tighter_and_boundary_matches_rank_higher() {
        assert!(s("dm", "dir_model.rs") > s("dm", "daemon_main.rs").map(|x| x - 100));
        assert!(s("main", "main.rs") > s("main", "a_big_amazing_interface_name.rs"));
    }

    #[test]
    fn smart_case() {
        assert!(s("Main", "main.rs").is_none());
        assert!(s("main", "Main.rs").is_some());
    }

    #[test]
    fn filters_parse() {
        let q = Query::parse("ext:RS type:f newer:1d foo");
        assert_eq!(q.ext, ["rs"]);
        assert_eq!(q.only_dirs, Some(false));
        assert!(q.newer.is_some() && q.has_terms());
        assert!(q.accepts("a.rs", false, 1, now()));
        assert!(!q.accepts("a.txt", false, 1, now()));
    }

    #[test]
    fn extension_lists_and_compound_extensions() {
        let q = Query::parse("ext:png,.JPG,tar.gz");
        assert!(q.accepts("a.png", false, 1, 0));
        assert!(q.accepts("photo.jpg", false, 1, 0));
        assert!(q.accepts("backup.tar.gz", false, 1, 0));
        assert!(!q.accepts("a.txt", false, 1, 0));
        assert!(!q.accepts(".png", false, 1, 0));
    }

    #[test]
    fn ordered_terms_like_zoxide() {
        assert!(ordered_terms_score("new tab", "New tab").is_some());
        assert!(ordered_terms_score("tab new", "New tab").is_none());
        assert!(ordered_terms_score("Tab", "New tab").is_none());
        assert!(ordered_terms_score("sp pan", "Split: add pane beside").is_some());
        assert!(
            ordered_terms_score("tab", "Tab one")
                > ordered_terms_score("tab", "Cycle pane layout table")
        );
    }

    #[test]
    fn regex_queries() {
        let q = Query::parse("re:^m.*\\.rs$");
        assert!(q.has_terms());
        assert!(q.score("main.rs", "src/main.rs").is_some());
        assert!(q.score("domain.rs", "src/domain.rs").is_none());
        // Smart case, and unfinished patterns match nothing.
        assert!(
            Query::parse("re:Main")
                .score("main.rs", "main.rs")
                .is_none()
        );
        assert!(Query::parse("re:(").score("main.rs", "main.rs").is_none());
    }

    #[test]
    fn ordered_words_outrank_scattered_matches() {
        let q = Query::parse("read me");
        let tight = q.score("read me first.txt", "read me first.txt").unwrap();
        let scattered = q.score("r_e_a_d_m_e.txt", "r_e_a_d_m_e.txt").unwrap();
        assert!(tight > scattered);
    }

    #[test]
    fn size_filters() {
        let q = Query::parse("size>1k size<1m");
        assert!(q.accepts("x", false, 2048, 0));
        assert!(!q.accepts("x", false, 10, 0));
    }
}

// ---- palette prediction ----------------------------------------------------------------------

/// Words that mean the same thing to someone typing into the palette. A query word also matches the
/// other words of any group it is in, a little less well than itself.
const SYNONYMS: &[&[&str]] = &[
    &[
        "delete", "remove", "erase", "trash", "bin", "rm", "destroy", "discard", "shred",
        "recycle", "rubbish", "garbage",
    ],
    &["copy", "duplicate", "clone", "yank"],
    &["move", "relocate", "cut", "mv", "transfer"],
    &["rename", "retitle"],
    &["create", "new", "make", "add"],
    &["folder", "directory", "dir"],
    &["hidden", "dotfiles", "invisible"],
    &["open", "launch", "run", "start"],
    &[
        "unmount",
        "disconnect",
        "eject",
        "umount",
        "detach",
        "unplug",
    ],
    &["connect", "login", "mount"],
    &[
        "settings",
        "preferences",
        "options",
        "config",
        "configure",
        "setup",
    ],
    &["preview", "inspector", "info", "details"],
    &["find", "search", "locate", "grep", "lookup"],
    &["refresh", "reload", "rescan", "update"],
    &["bookmark", "pin", "favorite", "favourite", "star"],
    &["network", "server", "remote", "share"],
    &["samba", "smb", "cifs", "windows"],
    &["ssh", "sftp", "scp"],
    &["terminal", "shell", "console", "cmd", "prompt"],
    &["quit", "exit", "leave"],
    &["undo", "revert"],
    &["redo", "repeat"],
    &["select", "mark", "tick", "check"],
    &["deselect", "unselect", "clear", "reset"],
    &["thumbnail", "icons", "gallery", "grid"],
    &["sidebar", "places", "drives"],
    &["play", "pause", "stop"],
    &["zoom", "bigger", "larger", "smaller", "magnify", "scale"],
    &["sort", "order", "arrange"],
    &["pane", "panel", "split"],
    &["compress", "zip", "archive", "pack"],
    &["extract", "unzip", "unpack", "uncompress", "untar"],
    &["path", "location", "address"],
    &["edit", "change", "modify", "view", "manage"],
    &["password", "credentials"],
    &["google", "gdrive"],
    &["aws", "amazon", "s3", "bucket"],
    &["webdav", "dav", "nextcloud", "owncloud"],
    &["onedrive", "microsoft"],
];

/// A loose word form: lowercase, with plural, `-ing`, `-ed`, `-er` and a final `e` taken off, so
/// "deleting", "deleted" and "delete" are the same word.
pub fn stem(word: &str) -> String {
    let mut w = word.to_lowercase();
    if w.chars().count() <= 3 {
        return w;
    }
    for _ in 0..2 {
        let mut next = None;
        if let Some(r) = w.strip_suffix("ies").filter(|r| r.len() >= 2) {
            next = Some(format!("{r}y"));
        } else {
            for suf in ["ing", "ed", "er", "es", "s"] {
                if suf == "s" && w.ends_with("ss") {
                    continue;
                }
                if let Some(r) = w.strip_suffix(suf).filter(|r| r.len() >= 3) {
                    let mut r = r.to_owned();
                    // "running" -> "runn" -> "run"
                    let b = r.as_bytes();
                    if suf == "ing" || suf == "ed" || suf == "er" {
                        if b.len() >= 4
                            && b[b.len() - 1] == b[b.len() - 2]
                            && !b"lsz".contains(&b[b.len() - 1])
                        {
                            r.pop();
                        }
                    }
                    next = Some(r);
                    break;
                }
            }
        }
        match next {
            Some(n) => w = n,
            None => break,
        }
    }
    if w.len() > 3 && w.ends_with('e') {
        w.pop();
    }
    w
}

/// Edits (insert, delete, replace, swap two neighbours) between two words.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=b.len() {
        d[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

fn text_words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Every word that shares a group with `word` (or with its word form).
fn synonyms_of(word: &str) -> Vec<&'static str> {
    let sw = stem(word);
    SYNONYMS
        .iter()
        .filter(|g| g.iter().any(|m| *m == word || stem(m) == sw))
        .flat_map(|g| g.iter().copied())
        .collect()
}

/// How well `q` matches the words of a command: 5 exact word, 4 start of a word, 3 same word form,
/// 2 synonym, 1 typo, 0 none.
fn word_tier(q: &str, words: &[String], stems: &[String]) -> u8 {
    if words.iter().any(|w| w == q) {
        return 5;
    }
    if words.iter().any(|w| w.starts_with(q)) {
        return 4;
    }
    if stems.iter().any(|s| *s == stem(q)) {
        return 3;
    }
    if synonyms_of(q)
        .iter()
        .any(|m| words.iter().any(|w| w == m) || stems.iter().any(|s| *s == stem(m)))
    {
        return 2;
    }
    let len = q.chars().count();
    if len < 4 {
        return 0;
    }
    let allowed = if len >= 7 { 2 } else { 1 };
    let near = |w: &str| {
        let wl = w.chars().count();
        wl >= 4 && wl.abs_diff(len) <= allowed && edit_distance(q, w) <= allowed
    };
    if words.iter().any(|w| near(w)) {
        return 1;
    }
    // A misspelt synonym ("prefrences" for settings): correct it against the known words first.
    let matches_text =
        |m: &str| words.iter().any(|w| w == m) || stems.iter().any(|s| *s == stem(m));
    let known = SYNONYMS
        .iter()
        .flat_map(|g| g.iter().copied())
        .filter(|m| near(m));
    for m in known {
        if matches_text(m) || synonyms_of(m).iter().any(|s| matches_text(s)) {
            return 1;
        }
    }
    0
}

/// `(weakest word tier, total)` when every word of `query` matches something in `text`, else `None`.
pub fn predict_score(query: &str, text: &str) -> Option<(u8, i32)> {
    let words = text_words(text);
    let stems: Vec<String> = words.iter().map(|w| stem(w)).collect();
    let mut weakest = 5;
    let mut total = 0;
    let mut any = false;
    for q in text_words(query) {
        any = true;
        let t = word_tier(&q, &words, &stems);
        if t == 0 {
            return None;
        }
        weakest = weakest.min(t);
        total += t as i32 * 10;
    }
    any.then(|| (weakest, total - text.len() as i32 / 16))
}

/// Order palette `items` (with command `ids`) for `query`: literal matches first; when there are none, the
/// best-guessed ones (word forms, synonyms, typos). Commands chosen for this query before come first,
/// and an empty query lists the most used commands first.
pub fn predict_rank(
    items: &[String],
    ids: &[String],
    query: &str,
    usage: &crate::state::Usage,
) -> Vec<usize> {
    let q = query.trim();
    let ql = q.to_lowercase();
    let count = |i: usize| {
        ids.get(i)
            .and_then(|id| usage.counts.get(id))
            .copied()
            .unwrap_or(0) as i32
    };
    if q.is_empty() {
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by(|&a, &b| count(b).cmp(&count(a)).then(a.cmp(&b)));
        return order;
    }
    // Learned: (is it an exact repeat of a query, bonus).
    let learned = |i: usize| -> (bool, i32) {
        let Some(id) = ids.get(i) else {
            return (false, 0);
        };
        let mut best = (false, 0);
        for p in usage.picks.iter().filter(|p| &p.id == id) {
            let cand = if p.q == ql {
                (true, 1000 + 100 * p.n as i32)
            } else if p.q.starts_with(&ql) {
                (false, 300 + 30 * p.n as i32)
            } else {
                continue;
            };
            if cand.1 > best.1 {
                best = cand;
            }
        }
        best
    };
    let mut scored: Vec<(i32, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| ordered_terms_score(q, s).map(|sc| (sc + learned(i).1, i)))
        .collect();
    let literal = scored.len();
    let guesses: Vec<(usize, u8, i32)> = items
        .iter()
        .enumerate()
        .filter(|(i, _)| !scored.iter().any(|s| s.1 == *i))
        .filter_map(|(i, s)| predict_score(q, s).map(|(t, sc)| (i, t, sc)))
        .collect();
    if literal == 0 {
        // Nothing literal: the best-matching tier of guesses, typos only as a last resort.
        let best = guesses.iter().map(|g| g.1).max().unwrap_or(0);
        scored = guesses
            .into_iter()
            .filter(|g| g.1 == best)
            .map(|(i, _, sc)| (sc + learned(i).1, i))
            .collect();
    } else {
        // Literal hits first; synonyms and word forms of what was typed follow them.
        scored.extend(
            guesses
                .into_iter()
                .filter(|g| g.1 >= 2)
                .map(|(i, _, sc)| (sc - 1000 + learned(i).1, i)),
        );
    }
    // A command picked for this query (or one it is the start of) shows up even when the words don't match.
    for i in 0..items.len() {
        let (_, bonus) = learned(i);
        if bonus > 0 && !scored.iter().any(|s| s.1 == i) {
            scored.push((bonus, i));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod predict_tests {
    use super::*;
    use crate::state::Usage;

    fn items(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn word_forms_collapse() {
        assert_eq!(stem("deleting"), stem("delete"));
        assert_eq!(stem("renaming"), stem("rename"));
        assert_eq!(stem("settings"), stem("setting"));
        assert_eq!(stem("copies"), stem("copy"));
        assert_eq!(stem("running"), stem("run"));
        assert_ne!(stem("copy"), stem("cut"));
    }

    #[test]
    fn typos_are_one_edit() {
        assert_eq!(edit_distance("disconect", "disconnect"), 1);
        assert_eq!(edit_distance("prefrences", "preferences"), 1);
        assert_eq!(edit_distance("recieve", "receive"), 1);
        assert_eq!(edit_distance("abc", "xyz"), 3);
    }

    #[test]
    fn guesses_find_the_command() {
        let list = items(&[
            "Close pane",
            "Move to trash",
            "Delete permanently…",
            "Unmount / disconnect / eject device…",
            "Settings…",
            "Pin / unpin this folder",
        ]);
        let ids: Vec<String> = (0..list.len()).map(|i| format!("c{i}")).collect();
        let u = Usage::default();
        let first = |q: &str| {
            predict_rank(&list, &ids, q, &u)
                .first()
                .map(|&i| list[i].clone())
        };
        assert_eq!(first("deleting").as_deref(), Some("Delete permanently…"));
        assert_eq!(first("preferences").as_deref(), Some("Settings…"));
        assert_eq!(first("prefrences").as_deref(), Some("Settings…"));
        assert_eq!(
            first("bookmark").as_deref(),
            Some("Pin / unpin this folder")
        );
        assert_eq!(
            first("disconect").as_deref(),
            Some("Unmount / disconnect / eject device…")
        );
        assert_eq!(first("zzzz"), None);
    }

    #[test]
    fn short_words_do_not_guess_wildly() {
        let list = items(&["Go to Downloads", "Cut"]);
        let ids = vec!["a".to_owned(), "b".to_owned()];
        assert!(predict_rank(&list, &ids, "cat", &Usage::default()).is_empty());
    }

    #[test]
    fn literal_matches_beat_guesses() {
        let list = items(&["Remove this", "Delete permanently…"]);
        let ids = vec!["a".to_owned(), "b".to_owned()];
        // The literal hit comes first; the synonym follows it.
        assert_eq!(
            predict_rank(&list, &ids, "remove", &Usage::default()),
            vec![0, 1]
        );
        assert_eq!(
            predict_rank(&list, &ids, "delete", &Usage::default()),
            vec![1, 0]
        );
    }

    #[test]
    fn learned_picks_rank_first_and_empty_query_orders_by_use() {
        let list = items(&["Move to trash", "Delete permanently…"]);
        let ids = vec!["trash".to_owned(), "delete".to_owned()];
        let mut u = Usage::default();
        assert_eq!(predict_rank(&list, &ids, "remove", &u), vec![0, 1]);
        u.record("remove", "delete", 1);
        u.record("remove", "delete", 2);
        assert_eq!(predict_rank(&list, &ids, "remove", &u), vec![1, 0]);
        assert_eq!(predict_rank(&list, &ids, "", &u), vec![1, 0]);
        // A prefix of a learned query leans the same way.
        assert_eq!(predict_rank(&list, &ids, "remo", &u).first(), Some(&1));
    }
}
