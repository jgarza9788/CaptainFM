//! "Open with": applications registered for a file's MIME type, via gio and desktop entries.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;

#[derive(Serialize, Debug, PartialEq)]
pub struct App {
    /// Desktop entry id, e.g. `org.gnome.Loupe.desktop`.
    pub id: String,
    pub name: String,
}

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate::state::home().join(".local/share")),
    ];
    let system =
        std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(
        system
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    dirs
}

fn desktop_file(id: &str) -> Option<PathBuf> {
    data_dirs()
        .into_iter()
        .map(|d| d.join("applications").join(id))
        .find(|p| p.is_file())
}

/// `Name=` of a desktop entry (the unlocalised one).
fn app_name(id: &str) -> Option<String> {
    let text = fs::read_to_string(desktop_file(id)?).ok()?;
    text.lines()
        .find_map(|l| l.strip_prefix("Name="))
        .map(str::to_owned)
}

/// Application ids from `gio mime <type>` output.
pub fn parse_gio_mime(text: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.ends_with(".desktop") && !l.contains(' ') && !ids.iter().any(|i| i == l) {
            ids.push(l.to_owned());
        }
    }
    ids
}

/// MIME type for a file `name` from a shared-mime-info `globs2` table (`weight:type:glob[:cs]`), by name
/// only. Highest weight wins, then the longest pattern ("*.tar.gz" over "*.gz").
pub fn mime_from_globs(name: &str, globs: &str) -> Option<String> {
    let lname = name.to_lowercase();
    let mut best: Option<(u32, usize, &str)> = None;
    for line in globs.lines().filter(|l| !l.starts_with('#')) {
        let mut parts = line.splitn(4, ':');
        let (Some(weight), Some(mime), Some(glob)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Ok(weight) = weight.parse::<u32>() else {
            continue;
        };
        let case_sensitive = parts
            .next()
            .is_some_and(|f| f.split(',').any(|f| f == "cs"));
        let (subject, pattern) = if case_sensitive {
            (name, glob.to_owned())
        } else {
            (lname.as_str(), glob.to_lowercase())
        };
        let hit = match pattern.strip_prefix('*') {
            Some(suffix) if !suffix.contains(['*', '?', '[']) => subject.ends_with(suffix),
            None if !pattern.contains(['*', '?', '[']) => subject == pattern,
            _ => false, // rarer wildcard patterns are left to gio
        };
        if hit && best.is_none_or(|(w, len, _)| (weight, glob.len()) > (w, len)) {
            best = Some((weight, glob.len(), mime));
        }
    }
    best.map(|(_, _, mime)| mime.to_owned())
}

/// The MIME type from the name alone, through the system's glob table.
fn guess_from_name(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    data_dirs()
        .into_iter()
        .filter_map(|d| fs::read_to_string(d.join("mime/globs2")).ok())
        .find_map(|globs| mime_from_globs(name, &globs))
}

/// On a remote, `gio info` reads the start of the file over the network to sniff its type, and this
/// runs on the UI thread (the context menu), so the name decides there.
fn content_type(path: &str) -> Option<String> {
    if crate::listing::is_remote(path) {
        return guess_from_name(path);
    }
    let out = Command::new("gio")
        .args(["info", "-a", "standard::content-type", path])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.trim().strip_prefix("standard::content-type:"))
        .map(|t| t.trim().to_owned())
}

pub fn apps_for(path: &str) -> Vec<App> {
    let Some(mime) = content_type(path) else {
        return Vec::new();
    };
    let Ok(out) = Command::new("gio").args(["mime", &mime]).output() else {
        return Vec::new();
    };
    parse_gio_mime(&String::from_utf8_lossy(&out.stdout))
        .into_iter()
        .filter_map(|id| app_name(&id).map(|name| App { id, name }))
        .collect()
}

/// Launch `app_id` with `path`; detached.
pub fn launch(app_id: &str, path: &str) {
    let Some(file) = desktop_file(app_id) else {
        return;
    };
    if let Ok(mut child) = Command::new("gio")
        .args(["launch"])
        .arg(file)
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_registered_and_recommended() {
        let text = "Registered applications:\n\torg.gnome.Loupe.desktop\n\tmpv.desktop\nRecommended applications:\n\tmpv.desktop\n";
        assert_eq!(
            parse_gio_mime(text),
            ["org.gnome.Loupe.desktop", "mpv.desktop"]
        );
        assert!(parse_gio_mime("No applications").is_empty());
    }

    #[test]
    fn mime_from_the_name_alone() {
        let globs = "# comment\n\
            50:application/pdf:*.pdf\n\
            50:application/gzip:*.gz\n\
            50:application/x-compressed-tar:*.tar.gz\n\
            50:image/png:*.png\n\
            40:image/apng:*.png\n\
            50:text/x-makefile:Makefile:cs\n\
            50:text/x-c:*.c:cs\n\
            50:text/x-c++:*.C:cs\n\
            10:text/x-readme:README*\n";
        let m = |n| mime_from_globs(n, globs);
        assert_eq!(
            m("Report.PDF").as_deref(),
            Some("application/pdf"),
            "case does not matter"
        );
        assert_eq!(
            m("backup.tar.gz").as_deref(),
            Some("application/x-compressed-tar"),
            "longest pattern wins"
        );
        assert_eq!(m("x.gz").as_deref(), Some("application/gzip"));
        assert_eq!(
            m("shot.png").as_deref(),
            Some("image/png"),
            "highest weight wins"
        );
        assert_eq!(m("Makefile").as_deref(), Some("text/x-makefile"));
        assert_eq!(
            m("makefile"),
            None,
            "a case-sensitive name must match exactly"
        );
        assert_eq!(
            (m("a.c").as_deref(), m("a.C").as_deref()),
            (Some("text/x-c"), Some("text/x-c++"))
        );
        assert_eq!(m("README.md"), None, "other wildcards are left to gio");
        assert_eq!(m("noext"), None);
    }

    #[test]
    fn the_system_glob_table_knows_common_types() {
        // Skips itself where shared-mime-info is not installed.
        if !std::path::Path::new("/usr/share/mime/globs2").exists() {
            return;
        }
        assert_eq!(
            guess_from_name("/x/y/photo.JPG").as_deref(),
            Some("image/jpeg")
        );
        assert_eq!(
            guess_from_name("/x/notes.txt").as_deref(),
            Some("text/plain")
        );
    }
}
