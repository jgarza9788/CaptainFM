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
    let Some(mut want) = it.next() else { return true };
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
    exts.iter().any(|e| lower.len() > e.len() + 1 && lower.ends_with(&format!(".{e}")))
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
            match regex::RegexBuilder::new(pattern).case_insensitive(insensitive).build() {
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

    /// Check the non-fuzzy filters. Unknown values (`size < 0`) never fail a filter.
    pub fn accepts(&self, name: &str, is_dir: bool, size: i64, mtime: i64) -> bool {
        if let Some(d) = self.only_dirs
            && d != is_dir {
                return false;
            }
        if !self.ext.is_empty() && !matches_ext(&self.ext, name) {
            return false;
        }
        if mtime > 0
            && (self.newer.is_some_and(|t| mtime < t) || self.older.is_some_and(|t| mtime > t)) {
                return false;
            }
        if size >= 0
            && (self.min_size.is_some_and(|m| size < m) || self.max_size.is_some_and(|m| size > m)) {
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
            && let Some(ordered) = ordered_terms_score(&self.words.join(" "), name) {
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
        let (hay, needle) = if sensitive { (text, term.to_owned()) } else { (lower.as_str(), term.to_lowercase()) };
        // Byte offsets can differ between `text` and `lower` for exotic Unicode; stay in range.
        let start = from.min(hay.len());
        if !hay.is_char_boundary(start) {
            return None;
        }
        let pos = start + hay[start..].find(&needle)?;
        let boundary = pos == 0 || !hay[..pos].chars().next_back().is_some_and(char::is_alphanumeric);
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
        assert!(ordered_terms_score("sp pan", "Split: add pane beside") .is_some());
        assert!(ordered_terms_score("tab", "Tab one") > ordered_terms_score("tab", "Cycle pane layout table"));
    }

    #[test]
    fn regex_queries() {
        let q = Query::parse("re:^m.*\\.rs$");
        assert!(q.has_terms());
        assert!(q.score("main.rs", "src/main.rs").is_some());
        assert!(q.score("domain.rs", "src/domain.rs").is_none());
        // Smart case, and unfinished patterns match nothing.
        assert!(Query::parse("re:Main").score("main.rs", "main.rs").is_none());
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
