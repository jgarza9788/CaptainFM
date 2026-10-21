//! Omarchy theme sync: colours from the active theme's `colors.toml`, reloaded live.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Command;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use crate::watch::{Watch, watch};

pub type Palette = BTreeMap<String, String>;

static PALETTE: OnceLock<RwLock<Palette>> = OnceLock::new();

/// The palette currently in effect (also used by Rust-side syntax highlighting).
pub fn palette() -> Palette {
    PALETTE
        .get_or_init(|| RwLock::new(load_palette()))
        .read()
        .map(|p| p.clone())
        .unwrap_or_default()
}

fn set_palette(p: Palette) {
    if let Ok(mut w) = PALETTE.get_or_init(|| RwLock::new(Palette::new())).write() {
        *w = p;
    }
}

fn theme_dir() -> PathBuf {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))
        .unwrap_or_default();
    state.join("omarchy/current")
}

/// Catppuccin Mocha, used when no Omarchy theme is present.
fn fallback() -> Palette {
    [
        ("mode", "dark"),
        ("accent", "#89b4fa"),
        ("selection", "#45475a"),
        ("muted", "#585b70"),
        ("background", "#1e1e2e"),
        ("dark_background", "#181825"),
        ("darker_background", "#11111b"),
        ("lighter_background", "#313244"),
        ("foreground", "#cdd6f4"),
        ("dark_foreground", "#6c7086"),
        ("light_foreground", "#bac2de"),
        ("red", "#f38ba8"),
        ("yellow", "#f9e2af"),
        ("orange", "#fab387"),
        ("green", "#a6e3a1"),
        ("cyan", "#94e2d5"),
        ("blue", "#89b4fa"),
        ("magenta", "#f5c2e7"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v.to_owned()))
    .collect()
}

/// Parse the flat `key = "value"` subset of TOML that Omarchy's colors.toml uses.
fn parse_colors(text: &str) -> Palette {
    let mut out = Palette::new();
    for raw in text.lines() {
        let Some((key, value)) = raw.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = match value.strip_prefix('"') {
            Some(rest) => rest.split('"').next().unwrap_or(""),
            // Unquoted: a comment starts at " #" (a leading # is part of a colour).
            None => value.split(" #").next().unwrap_or("").trim(),
        };
        out.insert(key.trim().to_owned(), value.to_owned());
    }
    out
}

fn load_palette() -> Palette {
    let mut palette = fallback();
    if let Ok(text) = std::fs::read_to_string(theme_dir().join("theme/colors.toml")) {
        palette.extend(parse_colors(&text));
    }
    palette
}

fn load_font() -> String {
    let from = |cmd: &str, args: &[&str]| {
        Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    from("omarchy-font-current", &[])
        .or_else(|| from("fc-match", &["-f", "%{family[0]}", "monospace"]))
        .unwrap_or_else(|| "monospace".to_owned())
}

#[cxx_qt::bridge]
mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    unsafe extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, json)]
        #[qproperty(QString, font_family, cxx_name = "fontFamily")]
        type Theme = super::ThemeRust;

        /// Load the theme and start watching for theme changes. Call once at startup.
        #[qinvokable]
        fn start(self: Pin<&mut Self>);
    }

    impl cxx_qt::Threading for Theme {}
}

#[derive(Default)]
pub struct ThemeRust {
    json: QString,
    font_family: QString,
    watcher: Option<Watch>,
}

impl qobject::Theme {
    fn start(mut self: Pin<&mut Self>) {
        self.as_mut().reload();

        let qt_thread = self.qt_thread();
        let dir = theme_dir();
        // Theme switches replace files under `current/`, so watch the whole subtree.
        let debouncer = watch(&dir, true, Duration::from_millis(150), move || {
            let _ = qt_thread.queue(|theme| theme.reload());
        });
        self.as_mut().rust_mut().watcher = debouncer;
    }

    fn reload(mut self: Pin<&mut Self>) {
        let palette = load_palette();
        let json = QString::from(serde_json::to_string(&palette).unwrap_or_default().as_str());
        if *self.json() != json {
            set_palette(palette);
            self.as_mut().set_json(json);
        }
        let font = QString::from(load_font().as_str());
        if *self.font_family() != font {
            self.as_mut().set_font_family(font);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Sandbox;

    #[test]
    fn parses_flat_toml_with_quotes_and_comments() {
        let p = parse_colors("mode = \"dark\"\n# a comment\naccent = \"#89b4fa\"\nmuted = #585b70 # trailing\n\nnot a pair\n");
        assert_eq!(p["mode"], "dark");
        assert_eq!(p["accent"], "#89b4fa");
        assert_eq!(p["muted"], "#585b70");
        assert!(!p.contains_key("not a pair"));
    }

    #[test]
    fn fallback_defines_every_role_the_ui_uses() {
        for role in ["accent", "selection", "background", "foreground", "red", "green", "yellow", "blue"] {
            assert!(fallback().contains_key(role), "missing {role}");
        }
    }

    #[test]
    fn active_theme_overrides_the_fallback() {
        let sb = Sandbox::new("theme");
        sb.touch(".local/state/omarchy/current/theme/colors.toml", "accent = \"#123456\"\n");
        let p = load_palette();
        assert_eq!(p["accent"], "#123456");
        // Roles the theme lacks keep their fallback value.
        assert_eq!(p["background"], fallback()["background"]);
    }

    #[test]
    fn missing_theme_uses_the_fallback() {
        let _sb = Sandbox::new("no-theme");
        assert_eq!(load_palette(), fallback());
    }
}
