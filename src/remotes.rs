//! Saved remote connections: SMB, SFTP, FTP, WebDAV and NFS through GVfs, and cloud storage through
//! `rclone mount`.
//!
//! Profiles (no secrets) live in `connections.json`. Passwords are only ever stored in the system
//! keyring (Secret Service, through `secret-tool`), and only when the user chose to remember them.
//! If there is no keyring the password is not saved; nothing is ever written in plain text.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::devices::{self, MountResult};
use crate::state;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// smb, sftp, ftp, dav, davs, nfs (GVfs) or gdrive, gcs, dropbox, onedrive, box, s3 (rclone).
    pub kind: String,
    pub host: String,
    pub port: String,
    /// Share (smb), folder, or bucket (gcs, s3).
    pub path: String,
    pub user: String,
    /// The password is in the keyring and is filled in on connect.
    pub remember: bool,
}

const GVFS_KINDS: &[&str] = &["smb", "sftp", "ftp", "dav", "davs", "nfs"];

pub fn is_cloud(kind: &str) -> bool {
    !GVFS_KINDS.contains(&kind)
}

// ---- profile store -------------------------------------------------------------------------

pub fn load() -> Vec<Profile> {
    serde_json::from_str(&state::read_config("connections")).unwrap_or_default()
}

fn store(list: &[Profile]) -> bool {
    state::write_config(
        "connections",
        &serde_json::to_string_pretty(list).unwrap_or_else(|_| "[]".into()),
    )
}

pub fn find(id: &str) -> Option<Profile> {
    load().into_iter().find(|p| p.id == id)
}

fn new_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("c{:x}", nanos & 0xffff_ffff_ffff)
}

/// Insert or replace a profile (matched by id), giving new ones an id. Returns the stored profile.
fn upsert(mut p: Profile) -> Profile {
    if p.id.is_empty() {
        p.id = new_id();
    }
    let mut list = load();
    match list.iter_mut().find(|q| q.id == p.id) {
        Some(q) => *q = p.clone(),
        None => list.push(p.clone()),
    }
    store(&list);
    p
}

/// Forget a connection: its profile, its keyring entry and (for cloud drives) its rclone remote.
pub fn delete(id: &str) -> MountResult {
    let Some(p) = find(id) else {
        return MountResult {
            ok: false,
            path: String::new(),
            message: "No such connection".into(),
        };
    };
    let mut list = load();
    list.retain(|q| q.id != id);
    store(&list);
    secret_clear(id);
    if is_cloud(&p.kind) {
        let _ = Command::new("rclone")
            .args(["config", "delete", &remote_name(&p)])
            .env("RCLONE_CONFIG", rclone_conf())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    MountResult {
        ok: true,
        path: String::new(),
        message: format!("Forgot {}", p.name),
    }
}

// ---- keyring -------------------------------------------------------------------------------

fn secret_store(id: &str, label: &str, password: &str) -> Result<(), String> {
    let mut child = Command::new("secret-tool")
        .args(["store", &format!("--label=CaptainFM: {label}"), "app", "captainfm", "profile", id])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "no keyring: install libsecret (secret-tool) and run a Secret Service such as gnome-keyring".to_owned())?;
    // The password goes over stdin so it never shows up in the process list.
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(password.as_bytes())
        .map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr)
            .trim()
            .lines()
            .last()
            .unwrap_or("the keyring refused it")
            .to_owned())
    }
}

pub fn secret_lookup(id: &str) -> Option<String> {
    let out = Command::new("secret-tool")
        .args(["lookup", "app", "captainfm", "profile", id])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    (out.status.success() && !out.stdout.is_empty())
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn secret_clear(id: &str) {
    let _ = Command::new("secret-tool")
        .args(["clear", "app", "captainfm", "profile", id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

// ---- GVfs ----------------------------------------------------------------------------------

/// Percent-encode the characters that would end a URL's user field.
fn encode_user(user: &str) -> String {
    let mut out = String::new();
    for c in user.chars() {
        match c {
            '@' | ':' | '/' | '%' | ' ' | '?' | '#' => out.push_str(&format!("%{:02X}", c as u32)),
            _ => out.push(c),
        }
    }
    out
}

/// The `scheme://user@host:port/path` a GVfs profile mounts.
pub fn gvfs_url(p: &Profile) -> String {
    let user = if p.user.is_empty() {
        String::new()
    } else {
        format!("{}@", encode_user(&p.user))
    };
    let port = if p.port.is_empty() {
        String::new()
    } else {
        format!(":{}", p.port)
    };
    let path = p.path.trim_matches('/');
    let path = if path.is_empty() {
        String::new()
    } else {
        format!("/{path}")
    };
    format!("{}://{user}{}{port}{path}", p.kind, p.host.trim())
}

/// The name a saved connection gives the mounted `uri` (as `gvfs_label` writes it), or `None` when no
/// saved connection matches its service, host and (for SMB) share. SMB connections saved without a
/// share show as `name/share`.
pub fn name_for(uri: &str, profiles: &[Profile]) -> Option<String> {
    let (scheme, rest) = uri.split_once("://")?;
    let scheme = match scheme {
        "http" => "dav",
        "https" => "davs",
        s => s,
    };
    let mut parts = rest.split('/');
    let (user, host) = match parts.next()?.rsplit_once('@') {
        Some((u, h)) => (u, h),
        None => ("", rest.split('/').next()?),
    };
    let host = host.split(':').next()?;
    let share = parts.next().unwrap_or("");
    let matches: Vec<&Profile> = profiles
        .iter()
        .filter(|p| {
            !is_cloud(&p.kind) && p.kind == scheme && p.host.trim().eq_ignore_ascii_case(host)
        })
        .filter(|p| {
            scheme != "smb"
                || p.path.trim_matches('/').is_empty()
                || p.path.trim_matches('/').eq_ignore_ascii_case(share)
        })
        .collect();
    // Saved under another service (an SMB share saved as SFTP): the same host and share/folder is still it.
    let by_path = || {
        profiles.iter().find(|p| {
            !is_cloud(&p.kind)
                && p.host.trim().eq_ignore_ascii_case(host)
                && !share.is_empty()
                && p.path.trim_matches('/').eq_ignore_ascii_case(share)
        })
    };
    let p = matches
        .iter()
        .find(|p| !user.is_empty() && p.user == user)
        .or(matches.first())
        .copied()
        .or_else(by_path)?;
    let name = if p.name.trim().is_empty() {
        return None;
    } else {
        p.name.trim()
    };
    if scheme == "smb" && p.path.trim_matches('/').is_empty() && !share.is_empty() {
        Some(format!("{name}/{share}"))
    } else {
        Some(name.to_owned())
    }
}

// ---- rclone --------------------------------------------------------------------------------

fn rclone_conf() -> PathBuf {
    state::config_dir().join("rclone.conf")
}

fn mounts_dir() -> PathBuf {
    state::data_dir().join("mounts")
}

/// A name that is safe in a path and in a mount table (no spaces or escapes).
pub fn slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_owned();
    if s.is_empty() { "drive".into() } else { s }
}

/// rclone remote name, also the middle of its `RCLONE_CONFIG_<NAME>_*` variables (letters and digits only).
pub fn remote_name(p: &Profile) -> String {
    format!(
        "cfm{}",
        p.id.chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
    )
}

/// Where a cloud drive mounts: its name for the eye and its id so two drives never share a folder.
pub fn mount_point(p: &Profile) -> PathBuf {
    mounts_dir().join(format!("{}-{}", slug(&p.name), slug(&p.id)))
}

/// Every folder a cloud drive may be mounted in, including the name-only one earlier versions used.
fn mount_points(p: &Profile) -> [PathBuf; 2] {
    [mount_point(p), mounts_dir().join(slug(&p.name))]
}

/// Is another saved connection already called `name` (ignoring case)?
fn name_taken(name: &str, except_id: &str) -> bool {
    let name = name.trim();
    load()
        .iter()
        .any(|q| q.id != except_id && q.name.trim().eq_ignore_ascii_case(name))
}

fn rclone_type(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "gdrive" => "drive",
        "gcs" => "google cloud storage",
        "dropbox" => "dropbox",
        "onedrive" => "onedrive",
        "box" => "box",
        "s3" => "s3",
        _ => return None,
    })
}

/// Remotes that need no browser sign-in: everything is passed through the environment, so the
/// secret key is never written to a file.
fn env_only(kind: &str) -> bool {
    kind == "s3"
}

/// Environment that defines the remote for kinds that are configured without a config file.
pub fn rclone_env(p: &Profile, password: &str) -> Vec<(String, String)> {
    let pre = format!("RCLONE_CONFIG_{}_", remote_name(p).to_uppercase());
    let mut env = vec![
        (format!("{pre}TYPE"), "s3".to_owned()),
        (format!("{pre}PROVIDER"), "Other".to_owned()),
    ];
    if !p.user.is_empty() {
        env.push((format!("{pre}ACCESS_KEY_ID"), p.user.clone()));
    }
    if !password.is_empty() {
        env.push((format!("{pre}SECRET_ACCESS_KEY"), password.to_owned()));
    }
    if !p.host.is_empty() {
        let endpoint = if p.host.contains("://") {
            p.host.clone()
        } else {
            format!("https://{}", p.host)
        };
        env.push((format!("{pre}ENDPOINT"), endpoint));
    }
    env
}

/// `(source, mount point)` of every rclone FUSE mount in a mount table.
pub fn parse_rclone_mounts(table: &str) -> Vec<(String, String)> {
    table
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            let (src, mp, fs) = (f.next()?, f.next()?, f.next()?);
            (fs == "fuse.rclone").then(|| (src.to_owned(), unescape_mount(mp)))
        })
        .collect()
}

/// `/proc/mounts` writes spaces and the like as octal escapes.
fn unescape_mount(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let oct = (b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c)))
        .then(|| u8::from_str_radix(&s[i + 1..i + 4], 8).ok())
        .flatten();
        match oct {
            Some(v) => {
                out.push(v);
                i += 4;
            }
            None => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Mounted cloud drives as (display name, mount point).
pub fn cloud_mounts() -> Vec<(String, String)> {
    let table = fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let profiles = load();
    parse_rclone_mounts(&table)
        .into_iter()
        .filter(|(_, mp)| Path::new(mp).starts_with(mounts_dir()))
        .map(|(src, mp)| {
            let name = profiles
                .iter()
                .find(|p| mount_points(p).iter().any(|m| m == Path::new(&mp)))
                .map_or_else(
                    || {
                        // Mounted by hand: the folder name says more than rclone's "cfm:".
                        Path::new(&mp).file_name().map_or_else(
                            || src.trim_end_matches(':').to_owned(),
                            |n| n.to_string_lossy().into_owned(),
                        )
                    },
                    |p| p.name.clone(),
                );
            (name, mp)
        })
        .collect()
}

fn is_cloud_mount(path: &str) -> bool {
    cloud_mounts().iter().any(|(_, mp)| mp == path)
}

/// Detach a FUSE mount, trying the tools in the order they usually exist.
fn fuse_unmount(path: &str) -> Result<(), String> {
    let mut last = String::from("could not unmount");
    for (cmd, args) in [
        ("fusermount3", ["-u", path]),
        ("fusermount", ["-u", path]),
        ("umount", [path, ""]),
    ] {
        let args: Vec<&str> = args.iter().copied().filter(|a| !a.is_empty()).collect();
        match Command::new(cmd).args(&args).stdin(Stdio::null()).output() {
            Ok(o) if o.status.success() => return Ok(()),
            Ok(o) => {
                last = String::from_utf8_lossy(&o.stderr)
                    .trim()
                    .lines()
                    .last()
                    .unwrap_or("failed")
                    .to_owned()
            }
            Err(_) => continue,
        }
    }
    Err(last)
}

/// Unmount a remote shown in the sidebar, whichever way it was mounted.
pub fn unmount(path: &str) -> MountResult {
    if is_cloud_mount(path) {
        return match fuse_unmount(path) {
            Ok(()) => {
                let _ = fs::remove_dir(path);
                MountResult {
                    ok: true,
                    path: String::new(),
                    message: "Disconnected".into(),
                }
            }
            Err(e) => MountResult {
                ok: false,
                path: String::new(),
                message: e,
            },
        };
    }
    devices::unmount_remote(path)
}

fn rclone(p: &Profile, password: &str) -> Command {
    let mut c = Command::new("rclone");
    c.env("RCLONE_CONFIG", rclone_conf()).stdin(Stdio::null());
    if env_only(&p.kind) {
        c.envs(rclone_env(p, password));
    }
    c
}

fn tail(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .trim()
        .lines()
        .last()
        .unwrap_or("")
        .to_owned()
}

fn fail(message: impl Into<String>) -> MountResult {
    MountResult {
        ok: false,
        path: String::new(),
        message: message.into(),
    }
}

/// Make sure the remote exists; for browser sign-in drives this opens the sign-in page and waits for it.
fn ensure_remote(p: &Profile, password: &str) -> Result<(), String> {
    if env_only(&p.kind) {
        return Ok(());
    }
    let name = remote_name(p);
    let listed = rclone(p, password)
        .arg("listremotes")
        .output()
        .map_err(|_| "rclone is not installed (get it at https://rclone.org/install/)".to_owned())?;
    if String::from_utf8_lossy(&listed.stdout)
        .lines()
        .any(|l| l.trim_end_matches(':') == name)
    {
        return Ok(());
    }
    let kind = rclone_type(&p.kind).ok_or("unknown service")?;
    let mut cmd = rclone(p, password);
    cmd.args(["config", "create", &name, kind]);
    match p.kind.as_str() {
        "gdrive" => {
            cmd.arg("scope=drive");
        }
        "gcs" if !p.host.is_empty() => {
            cmd.arg(format!("project_number={}", p.host));
        }
        _ => {}
    }
    let mut child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("rclone: {e}"))?;
    // Sign-in happens in the browser; give the person five minutes.
    let start = Instant::now();
    let done = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if start.elapsed() < Duration::from_secs(300) => {
                std::thread::sleep(Duration::from_millis(300))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Sign-in timed out".into());
            }
        }
    };
    // Sign-in tokens are kept in this file, so keep it to the owner.
    let _ = fs::set_permissions(rclone_conf(), fs::Permissions::from_mode(0o600));
    if done.success() {
        Ok(())
    } else {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut e, &mut err);
        }
        Err(err
            .trim()
            .lines()
            .last()
            .unwrap_or("sign-in failed")
            .to_owned())
    }
}

fn mount_cloud(p: &Profile, password: &str) -> MountResult {
    if let Err(e) = ensure_remote(p, password) {
        return fail(e);
    }
    let mp = mount_point(p);
    if let Some(m) = mount_points(p)
        .into_iter()
        .find(|m| cloud_mounts().iter().any(|(_, c)| Path::new(c) == m))
    {
        return MountResult {
            ok: true,
            path: m.to_string_lossy().into_owned(),
            message: "Connected".into(),
        };
    }
    if fs::create_dir_all(&mp).is_err() {
        return fail("could not create the mount folder");
    }
    let _ = fs::create_dir_all(state::cache_dir());
    let log = state::cache_dir().join(format!("rclone-{}.log", slug(&p.name)));
    let target = format!("{}:{}", remote_name(p), p.path.trim_matches('/'));
    let out = rclone(p, password)
        .arg("mount")
        .arg(&target)
        .arg(&mp)
        .args([
            "--daemon",
            "--vfs-cache-mode",
            "writes",
            "--dir-cache-time",
            "30s",
            "--log-level",
            "NOTICE",
            "--log-file",
        ])
        .arg(&log)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();
    let out = match out {
        Ok(o) => o,
        Err(_) => return fail("rclone is not installed (get it at https://rclone.org/install/)"),
    };
    if !out.status.success() {
        return fail(format!("rclone: {}", tail(&out.stderr)));
    }
    // --daemon returns once forked; wait for the mount to appear.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(20) {
        if cloud_mounts().iter().any(|(_, m)| Path::new(m) == mp) {
            return MountResult {
                ok: true,
                path: mp.to_string_lossy().into_owned(),
                message: "Connected".into(),
            };
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let why = fs::read_to_string(&log)
        .map(|t| tail(t.as_bytes()))
        .unwrap_or_default();
    fail(if why.is_empty() {
        "the drive did not mount".to_owned()
    } else {
        why
    })
}

// ---- connecting ----------------------------------------------------------------------------

/// What the person typed ahead of time, used to answer `gio mount` prompts without asking again.
pub struct Autofill {
    pub user: String,
    pub password: String,
}

/// Answer a `gio mount` prompt from `fill` once; `None` means ask the person.
pub fn autofill_answer(prompt: &str, fill: &mut Autofill) -> Option<String> {
    match devices::prompt_kind(prompt)? {
        "password" if !fill.password.is_empty() => Some(std::mem::take(&mut fill.password)),
        "user" if !fill.user.is_empty() => Some(std::mem::take(&mut fill.user)),
        // Take the suggested workgroup when credentials were given up front.
        "domain" if !fill.password.is_empty() || !fill.user.is_empty() => Some(String::new()),
        _ => None,
    }
}

/// Connect `profile`. A `password` is used for this connection; `save` stores the profile and, when
/// `profile.remember` is set, the password in the keyring.
pub fn connect(
    profile: Profile,
    password: String,
    save: bool,
    ask: impl Fn(&str) -> Option<String>,
) -> MountResult {
    if save {
        let name = if profile.name.trim().is_empty() {
            profile.host.trim()
        } else {
            profile.name.trim()
        };
        if !name.is_empty() && name_taken(name, &profile.id) {
            return fail(format!("A connection named {name} already exists"));
        }
    }
    let mut result = if is_cloud(&profile.kind) {
        mount_cloud(&profile, &password)
    } else {
        let url = gvfs_url(&profile);
        let fill = std::cell::RefCell::new(Autofill {
            user: String::new(),
            password: password.clone(),
        });
        devices::mount_uri(&url, |prompt| {
            autofill_answer(prompt, &mut fill.borrow_mut()).or_else(|| ask(prompt))
        })
    };
    if result.ok && save {
        let mut p = profile;
        if p.name.trim().is_empty() {
            p.name = if p.host.is_empty() {
                p.kind.clone()
            } else {
                p.host.clone()
            };
        }
        p.remember = p.remember && !password.is_empty();
        let mut note = String::new();
        // Never keep a password the person did not agree to keep.
        if p.remember {
            if p.id.is_empty() {
                p.id = new_id();
            }
            if let Err(e) = secret_store(&p.id, &p.name, &password) {
                p.remember = false;
                note = format!(" (password not saved: {e})");
            }
        }
        let stored = upsert(p);
        if !stored.remember {
            secret_clear(&stored.id);
        }
        result.message = format!("Connected to {}{note}", stored.name);
    }
    result
}

/// Change a saved connection (rename, new host, new password) without connecting. An empty `password`
/// keeps whatever the keyring already holds.
pub fn update(mut p: Profile, password: &str) -> MountResult {
    let Some(old) = find(&p.id) else {
        return fail("No such connection");
    };
    if p.name.trim().is_empty() {
        p.name = if p.host.is_empty() {
            p.kind.clone()
        } else {
            p.host.clone()
        };
    }
    if name_taken(&p.name, &p.id) {
        return fail(format!(
            "A connection named {} already exists",
            p.name.trim()
        ));
    }
    let mut note = String::new();
    // A mounted cloud drive lives in a folder named after the connection: let go of it before it changes.
    if is_cloud(&old.kind) && (old.name != p.name || old.kind != p.kind) {
        for mp in mount_points(&old) {
            let path = mp.to_string_lossy().into_owned();
            if !is_cloud_mount(&path) {
                continue;
            }
            match fuse_unmount(&path) {
                Ok(()) => {
                    let _ = fs::remove_dir(&mp);
                    note.push_str(" (disconnected; connect again to mount it)");
                }
                Err(e) => return fail(format!("Disconnect {} first: {e}", old.name)),
            }
        }
    }
    if p.remember {
        if !password.is_empty() {
            if let Err(e) = secret_store(&p.id, &p.name, password) {
                p.remember = false;
                note = format!(" (password not saved: {e})");
            }
        } else if secret_lookup(&p.id).is_none() {
            // Nothing to keep: turning "remember" on needs a password.
            p.remember = false;
        }
    }
    if !p.remember {
        secret_clear(&p.id);
    }
    let stored = upsert(p);
    MountResult {
        ok: true,
        path: String::new(),
        message: format!("Saved {}{note}", stored.name),
    }
}

/// Connect a saved profile, filling the password in from the keyring.
pub fn connect_saved(id: &str, ask: impl Fn(&str) -> Option<String>) -> MountResult {
    let Some(p) = find(id) else {
        return fail("No such connection");
    };
    let password = if p.remember {
        secret_lookup(id).unwrap_or_default()
    } else {
        String::new()
    };
    let mut r = connect(p.clone(), password, false, ask);
    if r.ok {
        r.message = format!("Connected to {}", p.name);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_named_drives_get_their_own_folder() {
        let mut a = profile("gdrive");
        let mut b = profile("gdrive");
        a.id = "c1".into();
        b.id = "c2".into();
        assert_ne!(mount_point(&a), mount_point(&b));
        // The folder earlier versions used is still recognised.
        assert!(mount_points(&a).contains(&mounts_dir().join(slug(&a.name))));
    }

    #[test]
    fn mounted_uris_take_the_saved_name() {
        let mut nas = profile("smb");
        nas.host = "192.168.1.250".into();
        let mut one = profile("smb");
        one.name = "Media".into();
        one.host = "192.168.1.250".into();
        one.path = "s01".into();
        let list = vec![one, nas];
        assert_eq!(
            name_for("smb://192.168.1.250/s01", &list).as_deref(),
            Some("Media")
        );
        assert_eq!(
            name_for("smb://192.168.1.250/s00", &list).as_deref(),
            Some("My NAS/s00")
        );
        assert_eq!(name_for("smb://other/s00", &list), None);
        assert_eq!(name_for("sftp://192.168.1.250", &list), None);
        // Saved as SFTP with folder s01, but mounted as an SMB share of that name.
        let mut odd = profile("sftp");
        odd.name = "S01".into();
        odd.host = "192.168.1.250".into();
        odd.path = "S01".into();
        assert_eq!(
            name_for("smb://root@192.168.1.250/s01", &[odd]).as_deref(),
            Some("S01")
        );
    }

    fn profile(kind: &str) -> Profile {
        Profile {
            id: "c1a2b".into(),
            name: "My NAS".into(),
            kind: kind.into(),
            host: "nas.local".into(),
            ..Default::default()
        }
    }

    #[test]
    fn gvfs_urls() {
        let mut p = profile("smb");
        p.path = "media".into();
        p.user = "bob".into();
        assert_eq!(gvfs_url(&p), "smb://bob@nas.local/media");
        let mut p = profile("sftp");
        p.port = "2222".into();
        p.user = "me@corp".into();
        p.path = "/srv/data/".into();
        assert_eq!(gvfs_url(&p), "sftp://me%40corp@nas.local:2222/srv/data");
        assert_eq!(gvfs_url(&profile("ftp")), "ftp://nas.local");
    }

    #[test]
    fn kinds_split_between_gvfs_and_rclone() {
        assert!(!is_cloud("sftp") && !is_cloud("smb"));
        assert!(is_cloud("gdrive") && is_cloud("gcs") && is_cloud("s3"));
        assert_eq!(rclone_type("gdrive"), Some("drive"));
        assert_eq!(rclone_type("sftp"), None);
    }

    #[test]
    fn s3_secret_goes_through_the_environment() {
        let mut p = profile("s3");
        p.user = "AKIA".into();
        p.host = "s3.example.com".into();
        let env = rclone_env(&p, "sekrit");
        let get = |k: &str| {
            env.iter()
                .find(|(n, _)| n.ends_with(k))
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("_SECRET_ACCESS_KEY"), Some("sekrit"));
        assert_eq!(get("_ACCESS_KEY_ID"), Some("AKIA"));
        assert_eq!(get("_ENDPOINT"), Some("https://s3.example.com"));
        assert!(env[0].0.starts_with("RCLONE_CONFIG_CFMC1A2B_"));
    }

    #[test]
    fn rclone_mounts_are_found_in_the_mount_table() {
        let table = "sysfs /sys sysfs rw 0 0\ngdrive: /home/u/My\\040Drive fuse.rclone rw,nosuid 0 0\n/dev/sda1 /mnt ext4 rw 0 0\n";
        assert_eq!(
            parse_rclone_mounts(table),
            [("gdrive:".to_owned(), "/home/u/My Drive".to_owned())]
        );
    }

    #[test]
    fn names_become_safe_folder_names() {
        assert_eq!(slug("My Drive (work)"), "My-Drive--work");
        assert_eq!(slug("///"), "drive");
    }

    #[test]
    fn prompts_are_answered_once_from_what_was_typed() {
        let mut fill = Autofill {
            user: String::new(),
            password: "pw".into(),
        };
        assert_eq!(
            autofill_answer("Password: ", &mut fill).as_deref(),
            Some("pw")
        );
        // A second password prompt means the first was wrong: ask the person.
        assert_eq!(autofill_answer("Password: ", &mut fill), None);
        assert_eq!(autofill_answer("Mounting...", &mut fill), None);
    }

    #[test]
    fn profiles_round_trip_without_secrets() {
        let _sb = crate::testutil::Sandbox::new("remotes-store");
        let mut p = profile("sftp");
        p.user = "bob".into();
        p.remember = true;
        let stored = upsert(p);
        assert!(stored.id.starts_with('c'));
        assert_eq!(find(&stored.id), Some(stored.clone()));
        let text = fs::read_to_string(state::config_dir().join("connections.json")).unwrap();
        assert!(!text.to_lowercase().contains("password\""), "{text}");
        assert!(delete(&stored.id).ok);
        assert!(load().is_empty());
        assert!(!delete(&stored.id).ok);
    }

    #[test]
    fn a_failed_connection_is_not_saved() {
        let _sb = crate::testutil::Sandbox::new("remotes-fail");
        let mut p = profile("ftp");
        p.host = "127.0.0.1".into();
        p.port = "1".into();
        let r = connect(p, "pw".into(), true, |_| None);
        assert!(!r.ok);
        assert!(load().is_empty());
    }
}
