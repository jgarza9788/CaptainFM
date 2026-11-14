//! File operations (copy, move, trash, rename, create) with an undo/redo journal.
//!
//! Trash follows the freedesktop spec under `$XDG_DATA_HOME/Trash`, so trashed files can be
//! restored by us (undo) or by any other file manager.

use crate::lock::MutexExt;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use serde::Serialize;

use crate::state::home;
use crate::transfer::{Ctl, Policy, State};

mod copy;
mod trash;
use copy::*;
pub use trash::*;

#[derive(Clone, Debug)]
enum Step {
    Moved {
        from: String,
        to: String,
    },
    Copied {
        from: String,
        to: String,
    },
    Linked {
        target: String,
        to: String,
    },
    Trashed {
        orig: String,
        stored: String,
        info: String,
    },
    CreatedFile {
        path: String,
    },
    CreatedDir {
        path: String,
    },
}

#[derive(Clone, Debug)]
struct Action {
    label: String,
    steps: Vec<Step>,
}

#[derive(Default)]
struct Journal {
    undo: Vec<Action>,
    redo: Vec<Action>,
}

static JOURNAL: Mutex<Journal> = Mutex::new(Journal {
    undo: Vec::new(),
    redo: Vec::new(),
});

#[derive(Serialize, Default, Debug)]
pub struct Outcome {
    pub label: String,
    pub ok: usize,
    pub errors: Vec<String>,
    /// Final paths of things created/moved, so the UI can put the cursor on them.
    pub paths: Vec<String>,
    /// Items left alone because the destination already had one and the answer was skip.
    pub skipped: usize,
    pub cancelled: bool,
    /// Remote items that were not trashed because the server has no trash: the UI offers to delete them.
    pub no_trash: Vec<String>,
    /// Whether undo can reverse this (trashing on a server cannot be undone from here).
    pub undo: bool,
}

impl Outcome {
    fn new(label: &str) -> Self {
        Outcome {
            label: label.to_owned(),
            ..Default::default()
        }
    }

    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

fn record(action: Action) {
    if action.steps.is_empty() {
        return;
    }
    let mut j = JOURNAL.locked();
    j.undo.push(action);
    j.redo.clear();
    let excess = j.undo.len().saturating_sub(200);
    j.undo.drain(..excess);
}

/// Make symbolic links to `paths` in `dest_dir` (named like the originals, `(copy)` when taken). Undoable.
pub fn link(paths: &[String], dest_dir: &str) -> Outcome {
    let mut out = Outcome::new("Linked");
    let mut steps = Vec::new();
    for src in paths {
        let dst = unique_dest(Path::new(dest_dir), &base_name(src));
        match symlink(src, &dst) {
            Ok(()) => {
                let to = dst.to_string_lossy().into_owned();
                out.paths.push(to.clone());
                steps.push(Step::Linked {
                    target: src.clone(),
                    to,
                });
                out.ok += 1;
            }
            Err(e) => out.errors.push(format!("{}: {e}", base_name(src))),
        }
    }
    record(Action {
        label: out.label.clone(),
        steps,
    });
    out
}

/// The file name a downloaded URL gets: its last path segment, decoded; `download` when there is none.
pub fn url_file_name(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let path = rest.split(['?', '#']).next().unwrap_or("");
    let path = path.split_once('/').map_or("", |(_, p)| p);
    let name = crate::dbus::percent_decode(path.rsplit('/').next().unwrap_or(""));
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && *c != '/')
        .collect();
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        "download".to_owned()
    } else {
        name.to_owned()
    }
}

/// Download http(s) URLs into `dest_dir` with curl, e.g. an image or link dragged out of a browser.
/// Not journaled: undo cannot bring a download back, and removing one is a plain delete.
pub fn download(urls: &[String], dest_dir: &str) -> Outcome {
    let mut out = Outcome::new("Downloaded");
    for url in urls {
        let dst = unique_dest(Path::new(dest_dir), &url_file_name(url));
        let status = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "300", "--proto", "=http,https", "-o"])
            .arg(&dst)
            .args(["--", url])
            .stdin(std::process::Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => {
                out.paths.push(dst.to_string_lossy().into_owned());
                out.ok += 1;
            }
            Ok(_) => {
                let _ = fs::remove_file(&dst);
                out.errors.push(format!(
                    "{}: download failed",
                    base_name(&dst.to_string_lossy())
                ));
            }
            Err(e) => out.errors.push(format!("curl: {e}")),
        }
    }
    out
}

/// An archive's name without its extension (`a.tar.gz` -> `a`), for the folder it extracts into.
pub fn archive_stem(name: &str) -> String {
    let mut stem = name;
    while let Some((head, ext)) = stem.rsplit_once('.') {
        let ext = ext.to_lowercase();
        let known = [
            "zip", "tar", "tgz", "gz", "xz", "bz2", "zst", "7z", "rar", "iso", "txz", "tbz2",
            "jar", "apk", "whl",
        ];
        if head.is_empty() || !known.contains(&ext.as_str()) {
            break;
        }
        stem = head;
    }
    stem.to_owned()
}

/// Extract archives with bsdtar, next to each archive: straight into its folder, or into a new
/// folder named after it. Existing files are kept. Not journaled: undo cannot tell what was new.
pub fn extract(paths: &[String], into_folder: bool) -> Outcome {
    let mut out = Outcome::new("Extracted");
    for src in paths {
        let name = base_name(src);
        let parent = Path::new(src).parent().unwrap_or(Path::new("/"));
        let dest = if into_folder {
            unique_dest(parent, &archive_stem(&name))
        } else {
            parent.to_path_buf()
        };
        if into_folder && let Err(e) = fs::create_dir(&dest) {
            out.errors.push(format!("{name}: {e}"));
            continue;
        }
        let result = Command::new("bsdtar")
            .args(["-xkf", src, "-C"])
            .arg(&dest)
            .stdin(std::process::Stdio::null())
            .output();
        match result {
            Ok(o) if o.status.success() => {
                if into_folder {
                    out.paths.push(dest.to_string_lossy().into_owned());
                }
                out.ok += 1;
            }
            Ok(o) => {
                if into_folder {
                    let _ = fs::remove_dir(&dest);
                }
                let msg = String::from_utf8_lossy(&o.stderr);
                out.errors.push(format!(
                    "{name}: {}",
                    msg.lines().next().unwrap_or("not an archive")
                ));
            }
            Err(e) => out.errors.push(format!("bsdtar: {e}")),
        }
    }
    out
}

// ---- helpers ---------------------------------------------------------------------------

fn base_name(p: &str) -> String {
    Path::new(p)
        .file_name()
        .map_or_else(|| p.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// `dir/name`, or `dir/name (copy)`, `dir/name (copy 2)`... when taken.
fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    for n in 1.. {
        let tag = if n == 1 {
            " (copy)".to_owned()
        } else {
            format!(" (copy {n})")
        };
        let candidate = dir.join(format!("{stem}{tag}{ext}"));
        if fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
    unreachable!()
}

// ---- public operations -----------------------------------------------------------------

/// Names in `paths` that `dest_dir` already has, so the user can choose what happens to them.
/// Copying into the folder an item is already in makes a `(copy)`, and moving it there does nothing,
/// so neither counts.
pub fn conflicts(paths: &[String], dest_dir: &str) -> Vec<String> {
    paths
        .iter()
        .filter(|src| Path::new(src).parent() != Some(Path::new(dest_dir)))
        .map(|src| base_name(src))
        .filter(|name| fs::symlink_metadata(Path::new(dest_dir).join(name)).is_ok())
        .collect()
}

/// Copy or move with the default answer to a name clash: keep both.
#[cfg(test)]
pub fn paste(paths: &[String], dest_dir: &str, cut: bool) -> Outcome {
    let ctl = Ctl::register(if cut { "Moved" } else { "Copied" }, dest_dir, paths.len());
    paste_tracked(&ctl, paths, dest_dir, cut, Policy::Both)
}

/// Copy or move as the transfer `ctl`, reporting progress through it.
pub fn paste_tracked(
    ctl: &Ctl,
    paths: &[String],
    dest_dir: &str,
    cut: bool,
    policy: Policy,
) -> Outcome {
    use std::os::unix::fs::MetadataExt;
    let mut out = Outcome::new(if cut { "Moved" } else { "Copied" });
    if ctl.cancelled() {
        out.cancelled = true;
        ctl.set_state(State::Cancelled);
        return out;
    }
    ctl.set_state(State::Running);
    let started = std::time::Instant::now();
    // Sizes for the progress bar. A move on one filesystem is a rename and has no bytes to count.
    let dest_dev = fs::metadata(dest_dir).map(|m| m.dev()).ok();
    let bytes: u64 = paths
        .iter()
        .filter(|p| {
            !cut || fs::symlink_metadata(p)
                .map(|m| Some(m.dev()) != dest_dev)
                .unwrap_or(false)
        })
        .map(|p| tree_bytes(Path::new(p)))
        .sum();
    ctl.bytes_total
        .store(bytes, std::sync::atomic::Ordering::Relaxed);
    let mut steps = Vec::new();
    for src in paths {
        let name = base_name(src);
        ctl.set_current(&name);
        let same_folder = Path::new(src).parent() == Some(Path::new(dest_dir));
        // Moving into the folder it is already in is a no-op.
        if cut && same_folder {
            ctl.items_done
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continue;
        }
        let wanted = Path::new(dest_dir).join(&name);
        let clash = !same_folder && fs::symlink_metadata(&wanted).is_ok();
        let mut replaced = None;
        let dst = if clash && policy == Policy::Skip {
            out.skipped += 1;
            ctl.items_done
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            continue;
        } else if clash && policy == Policy::Replace {
            if Path::new(src).starts_with(&wanted) {
                out.errors.push(format!(
                    "{name}: cannot replace a folder with something inside it"
                ));
                continue;
            }
            // The old one goes to the trash, so replacing is undoable (on a remote: see `clear_for_replace`).
            match clear_for_replace(&wanted.to_string_lossy()) {
                Ok(step) => replaced = step,
                Err(e) => {
                    out.errors.push(format!("{name}: {e}"));
                    continue;
                }
            }
            wanted
        } else {
            unique_dest(Path::new(dest_dir), &name)
        };
        let result = if cut {
            move_tracked(Path::new(src), &dst, ctl)
        } else {
            copy_tracked(Path::new(src), &dst, ctl)
        };
        match result {
            Ok(()) => {
                let (from, to) = (src.clone(), dst.to_string_lossy().into_owned());
                out.paths.push(to.clone());
                steps.extend(replaced);
                steps.push(if cut {
                    Step::Moved { from, to }
                } else {
                    Step::Copied { from, to }
                });
                out.ok += 1;
                ctl.items_done
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                // Do not leave half a copy behind, and put back what was replaced.
                let stopped = e.kind() == io::ErrorKind::Interrupted;
                if fs::symlink_metadata(&dst).is_ok() {
                    let _ = remove_any(&dst);
                }
                if let Some(Step::Trashed { orig, stored, info }) = replaced {
                    let _ = restore_one(&orig, &stored, &info);
                }
                if stopped {
                    out.cancelled = true;
                    break;
                }
                out.errors.push(format!("{name}: {e}"));
            }
        }
    }
    ctl.set_errors(out.errors.len(), out.errors.first().map_or("", |e| e));
    ctl.set_state(if out.cancelled {
        State::Cancelled
    } else if out.ok == 0 && !out.errors.is_empty() {
        State::Failed
    } else {
        State::Done
    });
    if std::env::var_os("CFM_DEBUG").is_some() {
        eprintln!(
            "[timing] {} {} items, {} bytes: {:?}",
            out.label.to_lowercase(),
            out.ok,
            ctl.bytes_done.load(std::sync::atomic::Ordering::Relaxed),
            started.elapsed()
        );
    }
    record(Action {
        label: out.label.clone(),
        steps,
    });
    out
}

/// Permanent deletion. Not undoable.
pub fn delete(paths: &[String]) -> Outcome {
    let mut out = Outcome::new("Deleted");
    for p in paths {
        match remove_any(Path::new(p)) {
            Ok(()) => out.ok += 1,
            Err(e) => out.errors.push(format!("{}: {e}", base_name(p))),
        }
    }
    out
}

pub fn rename(path: &str, new_name: &str) -> Outcome {
    let mut out = Outcome::new("Renamed");
    if new_name.is_empty() || new_name.contains('/') {
        out.errors.push("invalid name".into());
        return out;
    }
    let Some(parent) = Path::new(path).parent() else {
        out.errors.push("no parent folder".into());
        return out;
    };
    let dst = parent.join(new_name);
    if dst == Path::new(path) {
        return out;
    }
    if fs::symlink_metadata(&dst).is_ok() {
        out.errors.push(format!("{new_name} already exists"));
        return out;
    }
    match fs::rename(path, &dst) {
        Ok(()) => {
            let to = dst.to_string_lossy().into_owned();
            out.ok = 1;
            out.paths.push(to.clone());
            record(Action {
                label: out.label.clone(),
                steps: vec![Step::Moved {
                    from: path.to_owned(),
                    to,
                }],
            });
        }
        Err(e) => out.errors.push(e.to_string()),
    }
    out
}

/// Apply many renames at once as one undoable action. Pairs are `(from, to)` full paths.
pub fn rename_many(pairs: &[(String, String)]) -> Outcome {
    let mut out = Outcome::new("Renamed");
    let mut steps = Vec::new();
    // Two-phase through temporary names so swaps and shifts (a->b, b->c) work.
    let mut staged = Vec::new();
    for (i, (from, to)) in pairs.iter().enumerate() {
        if from == to {
            continue;
        }
        let tmp = format!("{from}.cfm-tmp-{i}");
        match fs::rename(from, &tmp) {
            Ok(()) => staged.push((from.clone(), tmp, to.clone())),
            Err(e) => out.errors.push(format!("{}: {e}", base_name(from))),
        }
    }
    for (from, tmp, to) in staged {
        if fs::symlink_metadata(&to).is_ok() {
            let _ = fs::rename(&tmp, &from);
            out.errors
                .push(format!("{}: target exists", base_name(&to)));
            continue;
        }
        match fs::rename(&tmp, &to) {
            Ok(()) => {
                out.ok += 1;
                out.paths.push(to.clone());
                steps.push(Step::Moved { from, to });
            }
            Err(e) => {
                let _ = fs::rename(&tmp, &from);
                out.errors.push(format!("{}: {e}", base_name(&to)));
            }
        }
    }
    record(Action {
        label: out.label.clone(),
        steps,
    });
    out
}

pub fn create(dir: &str, name: &str, is_dir: bool) -> Outcome {
    let mut out = Outcome::new(if is_dir {
        "Created folder"
    } else {
        "Created file"
    });
    if name.is_empty() || name.contains('/') {
        out.errors.push("invalid name".into());
        return out;
    }
    let path = Path::new(dir).join(name);
    let result = if is_dir {
        fs::create_dir(&path)
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map(|_| ())
    };
    match result {
        Ok(()) => {
            let p = path.to_string_lossy().into_owned();
            out.ok = 1;
            out.paths.push(p.clone());
            let step = if is_dir {
                Step::CreatedDir { path: p }
            } else {
                Step::CreatedFile { path: p }
            };
            record(Action {
                label: out.label.clone(),
                steps: vec![step],
            });
        }
        Err(e) => out.errors.push(e.to_string()),
    }
    out
}

fn undo_step(step: &Step) -> io::Result<Step> {
    match step {
        Step::Moved { from, to } => {
            if fs::symlink_metadata(from).is_ok() {
                return Err(io::Error::other(format!("{from} already exists")));
            }
            move_any(Path::new(to), Path::new(from))?;
            Ok(step.clone())
        }
        Step::Copied { from: _, to } => {
            // Undoing a copy moves the copy to the trash rather than deleting it outright.
            unmake(to, false)?;
            Ok(step.clone())
        }
        Step::Linked { to, .. } => {
            unmake(to, false)?;
            Ok(step.clone())
        }
        Step::Trashed { orig, stored, info } => {
            restore_one(orig, stored, info)?;
            Ok(step.clone())
        }
        Step::CreatedFile { path } | Step::CreatedDir { path } => {
            unmake(path, true)?;
            Ok(step.clone())
        }
    }
}

fn redo_step(step: &Step) -> io::Result<Step> {
    match step {
        Step::Moved { from, to } => {
            move_any(Path::new(from), Path::new(to))?;
            Ok(step.clone())
        }
        Step::Copied { from, to } => {
            copy_recursive(Path::new(from), Path::new(to))?;
            Ok(step.clone())
        }
        Step::Linked { target, to } => {
            symlink(target, to)?;
            Ok(step.clone())
        }
        Step::Trashed { orig, .. } => trash_one(orig, &deletion_date()),
        Step::CreatedFile { path } => {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            Ok(step.clone())
        }
        Step::CreatedDir { path } => {
            fs::create_dir(path)?;
            Ok(step.clone())
        }
    }
}

fn step_paths(step: &Step) -> Vec<String> {
    match step {
        Step::Moved { from, to } => vec![from.clone(), to.clone()],
        Step::Copied { to, .. } | Step::Linked { to, .. } => vec![to.clone()],
        Step::Trashed { orig, .. } => vec![orig.clone()],
        Step::CreatedFile { path } | Step::CreatedDir { path } => vec![path.clone()],
    }
}

/// Undo (`redo == false`) or redo the latest journaled action.
pub fn step_history(redo: bool) -> Outcome {
    let action = {
        let mut j = JOURNAL.locked();
        if redo { j.redo.pop() } else { j.undo.pop() }
    };
    let Some(action) = action else {
        let mut out = Outcome::new(if redo {
            "Nothing to redo"
        } else {
            "Nothing to undo"
        });
        out.errors.push(out.label.clone());
        return out;
    };
    let mut out = Outcome::new(&format!(
        "{} {}",
        if redo { "Redid" } else { "Undid" },
        action.label.to_lowercase()
    ));
    let mut done = Vec::new();
    // Undo walks steps in reverse so dependent steps unwind in order.
    let ordered: Vec<&Step> = if redo {
        action.steps.iter().collect()
    } else {
        action.steps.iter().rev().collect()
    };
    for step in ordered {
        match if redo {
            redo_step(step)
        } else {
            undo_step(step)
        } {
            Ok(new_step) => {
                out.ok += 1;
                out.paths.extend(step_paths(step));
                done.push(new_step);
            }
            Err(e) => out.errors.push(e.to_string()),
        }
    }
    if !redo {
        done.reverse();
    }
    let inverse = Action {
        label: action.label,
        steps: done,
    };
    let mut j = JOURNAL.locked();
    if inverse.steps.is_empty() {
        return out;
    }
    if redo {
        j.undo.push(inverse)
    } else {
        j.redo.push(inverse)
    }
    out
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod journal_tests;
