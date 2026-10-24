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

fn content_type(path: &str) -> Option<String> {
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
}
