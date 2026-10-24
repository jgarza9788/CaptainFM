//! Devices and remotes: disks via lsblk/udisksctl (UDisks2) and network or phone mounts via GVfs.

use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Device {
    /// `disk:/dev/sdb1`, `remote:<gvfs entry>` or `mtp:<activation uri>`.
    pub id: String,
    pub kind: &'static str,
    pub label: String,
    pub detail: String,
    pub mounted: bool,
    /// Mount point (or GVfs path) when mounted.
    pub path: String,
    pub device: String,
    pub uri: String,
    pub removable: bool,
    /// Percent of the filesystem in use, when mounted.
    pub usage: i32,
    /// Bytes in use and in total (like `df`), 0 when unknown.
    pub used: u64,
    pub total: u64,
}

/// Call `on_change` (from a helper thread) whenever a disk or share is mounted or unmounted, so the
/// sidebar updates when it happens instead of on a timer. The kernel wakes us when `/proc/self/mountinfo`
/// changes; gvfs mounts live inside their own folder, which is watched separately.
pub fn watch_changes(on_change: impl Fn() + Send + Sync + 'static) {
    use std::io::{Read, Seek};
    use std::os::fd::AsRawFd;
    let on_change = std::sync::Arc::new(on_change);
    let gvfs = {
        let on_change = on_change.clone();
        crate::watch::watch(
            &gvfs_dir(),
            false,
            std::time::Duration::from_millis(300),
            move || on_change(),
        )
    };
    std::thread::spawn(move || {
        let _keep_watching = gvfs;
        let Ok(mut file) = fs::File::open("/proc/self/mountinfo") else {
            return;
        };
        let mut text = String::new();
        loop {
            // Reading it all re-arms the notification.
            let _ = file.rewind();
            text.clear();
            let _ = file.read_to_string(&mut text);
            let mut poll = libc::pollfd {
                fd: file.as_raw_fd(),
                events: libc::POLLPRI,
                revents: 0,
            };
            // SAFETY: one valid pollfd, and the descriptor stays open for the whole call.
            if unsafe { libc::poll(&mut poll, 1, -1) } < 0 {
                return;
            }
            // A mount often comes with several changes in a burst.
            std::thread::sleep(std::time::Duration::from_millis(300));
            on_change();
        }
    });
}

fn gvfs_dir() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    runtime.join("gvfs")
}

// ---- parsing (pure, unit-tested) -----------------------------------------------------------

/// Filesystems hidden from the list: containers rather than something you can open.
const CONTAINER_FS: &[&str] = &[
    "swap",
    "crypto_LUKS",
    "LVM2_member",
    "linux_raid_member",
    "zfs_member",
];

fn collect_block(node: &Value, out: &mut Vec<Device>) {
    let s = |k: &str| node.get(k).and_then(Value::as_str).unwrap_or("").to_owned();
    let fstype = s("fstype");
    let mount = node
        .get("mountpoints")
        .and_then(Value::as_array)
        // A filesystem can be mounted several times (btrfs subvolumes): the shortest path is the root.
        .and_then(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .filter(|m| m.starts_with('/'))
                .min_by_key(|m| m.len())
        })
        .unwrap_or("")
        .to_owned();
    let removable = node.get("rm").and_then(Value::as_bool).unwrap_or(false)
        || node
            .get("hotplug")
            .and_then(Value::as_bool)
            .unwrap_or(false);

    if !fstype.is_empty() && !CONTAINER_FS.contains(&fstype.as_str()) {
        let label = [s("label"), s("model"), s("name")]
            .into_iter()
            .find(|l| !l.is_empty())
            .unwrap_or_default();
        out.push(Device {
            id: format!("disk:{}", s("path")),
            kind: "disk",
            label,
            detail: format!("{} {}", s("size"), fstype).trim().to_owned(),
            mounted: !mount.is_empty(),
            path: mount,
            device: s("path"),
            uri: String::new(),
            removable,
            usage: -1,
            used: 0,
            total: 0,
        });
    }
    if let Some(children) = node.get("children").and_then(Value::as_array) {
        for child in children {
            collect_block(child, out);
        }
    }
}

pub fn parse_lsblk(json: &str) -> Vec<Device> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for node in v
        .get("blockdevices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        collect_block(node, &mut out);
    }
    out
}

/// `sftp:host=example.com,user=bob` -> (`sftp`, {host, user}).
fn split_gvfs_name(entry: &str) -> (String, Vec<(String, String)>) {
    let (scheme, rest) = entry.split_once(':').unwrap_or((entry, ""));
    let params = rest
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    (scheme.to_owned(), params)
}

/// Human-readable URI for a GVfs mount directory name.
pub fn gvfs_label(entry: &str) -> String {
    let (scheme, params) = split_gvfs_name(entry);
    let get = |k: &str| {
        params
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.as_str())
            .unwrap_or("")
    };
    match scheme.as_str() {
        "smb-share" => format!("smb://{}/{}", get("server"), get("share")),
        "sftp" | "ftp" | "dav" | "davs" | "afp-volume" => {
            let user = get("user");
            let scheme = if scheme == "dav" {
                "http"
            } else if scheme == "davs" {
                "https"
            } else {
                &scheme
            };
            if user.is_empty() {
                format!("{scheme}://{}", get("host"))
            } else {
                format!("{scheme}://{user}@{}", get("host"))
            }
        }
        _ => format!("{scheme}://{}", get("host")),
    }
}

pub fn gvfs_kind(entry: &str) -> &'static str {
    let scheme = entry.split(':').next().unwrap_or("");
    if matches!(scheme, "mtp" | "gphoto2" | "afc") {
        "mtp"
    } else {
        "remote"
    }
}

/// Parse `gio mount -li` for volumes that can be mounted but are not yet (phones, cameras).
pub fn parse_gio_volumes(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Volume(") {
            current = rest.split_once("): ").map(|(_, n)| n.trim().to_owned());
        } else if !line.starts_with(' ') {
            current = None;
        } else if let (Some(name), Some(uri)) =
            (&current, line.trim().strip_prefix("activation_root="))
            && ["mtp://", "gphoto2://", "afc://"]
                .iter()
                .any(|p| uri.starts_with(p))
        {
            out.push((name.clone(), uri.to_owned()));
        }
    }
    out
}

/// Parse `gio mount -l` for mounts gvfs knows about (name, uri), skipping local `file://` ones.
pub fn parse_gio_mounts(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim_start().strip_prefix("Mount(")?;
            let (_, rest) = rest.split_once("): ")?;
            let (name, uri) = rest.rsplit_once(" -> ")?;
            let uri = uri.trim();
            (uri.contains("://") && !uri.starts_with("file://"))
                .then(|| (name.trim().to_owned(), uri.to_owned()))
        })
        .collect()
}

/// Path reported by `udisksctl mount`: "Mounted /dev/sdb1 at /run/media/u/DISK."
pub fn parse_udisks_mount(output: &str) -> Option<String> {
    let (_, rest) = output.split_once(" at ")?;
    Some(rest.trim().trim_end_matches('.').to_owned())
}

// ---- listing -------------------------------------------------------------------------------

fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.trim().lines().last().unwrap_or("failed").to_owned())
    }
}

/// Bytes used and total of a mounted filesystem (like `df`), or `None`.
pub fn space(path: &str) -> Option<(u64, u64)> {
    let c = std::ffi::CString::new(path).ok()?;
    // SAFETY: statvfs only writes into the zeroed struct we pass.
    unsafe {
        let mut st: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(c.as_ptr(), &mut st) != 0 || st.f_blocks == 0 {
            return None;
        }
        let unit = if st.f_frsize > 0 {
            st.f_frsize as u64
        } else {
            st.f_bsize as u64
        };
        let used = (st.f_blocks - st.f_bfree) as u64 * unit;
        Some((used, used + st.f_bavail as u64 * unit))
    }
}

/// Bytes used and total from the output of `gio info -f <path>` (`filesystem::size` and `filesystem::free`), or `None`
/// when either is missing or the size is zero (some servers report nothing).
pub fn parse_gio_space(text: &str) -> Option<(u64, u64)> {
    let value = |key: &str| -> Option<u64> {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key)?.trim().parse().ok())
    };
    let total = value("filesystem::size:")?;
    let free = value("filesystem::free:")?;
    (total > 0).then(|| (total.saturating_sub(free), total))
}

/// Run a program and return what it printed, giving up (and killing it) after `limit`.
fn run_limited(cmd: &str, args: &[&str], limit: std::time::Duration) -> Option<String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(15)),
        }
    }
    let out = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// How full a remote or cloud drive is: bytes used and total. gvfs mounts (SMB, SFTP, FTP, WebDAV, NFS) are asked through
/// `gio info -f`, which reports the server's numbers; anything else (an rclone mount) through statvfs. One remote at a time,
/// and only when someone looks at it, since it is a network round trip that can hang.
pub fn remote_space(path: &str) -> Option<(u64, u64)> {
    if std::path::Path::new(path).starts_with(gvfs_dir()) {
        parse_gio_space(&run_limited(
            "gio",
            &["info", "-f", path],
            std::time::Duration::from_secs(4),
        )?)
    } else {
        space(path)
    }
}

/// Answers from `remote_space`, kept for a minute so hovering a row again costs nothing. A failed lookup is kept too, so a
/// dead server is not asked again every time the pointer passes over it.
#[derive(Default)]
pub struct SpaceCache {
    map: std::collections::HashMap<String, (std::time::Instant, Option<(u64, u64)>)>,
}

impl SpaceCache {
    pub const FRESH: std::time::Duration = std::time::Duration::from_secs(60);

    /// `Some(answer)` when there is a fresh one (the answer itself may be `None`: nothing reported).
    pub fn get(&self, path: &str, now: std::time::Instant) -> Option<Option<(u64, u64)>> {
        let (at, value) = self.map.get(path)?;
        (now.saturating_duration_since(*at) < Self::FRESH).then_some(*value)
    }

    pub fn put(&mut self, path: &str, now: std::time::Instant, value: Option<(u64, u64)>) {
        self.map.insert(path.to_owned(), (now, value));
    }
}

/// Percent of a mounted filesystem that is used (like `df`), or -1.
pub fn usage_percent(path: &str) -> i32 {
    space(path).map_or(-1, |(used, total)| {
        ((used as f64 / total.max(1) as f64) * 100.0).round() as i32
    })
}

pub fn list() -> Vec<Device> {
    let mut devices = run(
        "lsblk",
        &[
            "-J",
            "-o",
            "NAME,PATH,LABEL,SIZE,TYPE,MOUNTPOINTS,FSTYPE,RM,HOTPLUG,MODEL,PKNAME",
        ],
    )
    .map(|j| parse_lsblk(&j))
    .unwrap_or_default();

    for d in devices.iter_mut().filter(|d| d.mounted) {
        if let Some((used, total)) = space(&d.path) {
            d.used = used;
            d.total = total;
        }
        d.usage = usage_percent(&d.path);
    }

    let profiles = crate::remotes::load();
    let mut mounted_hosts: Vec<String> = Vec::new();
    if let Ok(rd) = fs::read_dir(gvfs_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            mounted_hosts.push(name.clone());
            devices.push(Device {
                id: format!("remote:{name}"),
                kind: gvfs_kind(&name),
                label: crate::remotes::name_for(&gvfs_label(&name), &profiles)
                    .unwrap_or_else(|| gvfs_label(&name)),
                detail: "mounted".into(),
                mounted: true,
                path: e.path().to_string_lossy().into_owned(),
                device: String::new(),
                uri: gvfs_label(&name),
                removable: true,
                usage: -1,
                used: 0,
                total: 0,
            });
        }
    }
    for (name, mp) in crate::remotes::cloud_mounts() {
        devices.push(Device {
            id: format!("cloud:{mp}"),
            kind: "cloud",
            label: name,
            detail: "cloud drive".into(),
            mounted: true,
            path: mp,
            device: String::new(),
            uri: String::new(),
            removable: true,
            usage: -1,
            used: 0,
            total: 0,
        });
    }
    if let Ok(text) = run("gio", &["mount", "-li"]) {
        // Mounts gvfs holds without a FUSE directory would otherwise be invisible here.
        let mut claimed: Vec<String> = Vec::new();
        let mut orphans: Vec<String> = Vec::new();
        for (_, uri) in parse_gio_mounts(&text) {
            match find_gvfs_entry(&uri) {
                Some((entry, _)) => claimed.push(entry),
                None => orphans.push(uri),
            }
        }
        for uri in orphans {
            // The same mount under another host spelling (name versus IP) already has its row.
            if let Some(i) = twin_entry(&mounted_hosts, &claimed, &uri) {
                claimed.push(mounted_hosts[i].clone());
                continue;
            }
            devices.push(Device {
                id: format!("remote:{uri}"),
                kind: "remote",
                label: crate::remotes::name_for(&uri, &profiles).unwrap_or_else(|| uri.clone()),
                detail: "mounted".into(),
                mounted: true,
                path: String::new(),
                device: String::new(),
                uri,
                removable: true,
                usage: -1,
                used: 0,
                total: 0,
            });
        }
        for (name, uri) in parse_gio_volumes(&text) {
            let host = uri
                .split("host=")
                .nth(1)
                .unwrap_or("")
                .trim_end_matches('/')
                .to_owned();
            if !host.is_empty() && mounted_hosts.iter().any(|m| m.contains(&host)) {
                continue;
            }
            devices.push(Device {
                id: format!("mtp:{uri}"),
                kind: "mtp",
                label: name,
                detail: "not mounted".into(),
                mounted: false,
                path: String::new(),
                device: String::new(),
                uri,
                removable: true,
                usage: -1,
                used: 0,
                total: 0,
            });
        }
    }
    devices
}

// ---- mounting ------------------------------------------------------------------------------

#[derive(Serialize)]
pub struct MountResult {
    pub ok: bool,
    pub path: String,
    pub message: String,
}

fn done(path: String, message: &str) -> MountResult {
    MountResult {
        ok: true,
        path,
        message: message.to_owned(),
    }
}

fn failed(message: String) -> MountResult {
    MountResult {
        ok: false,
        path: String::new(),
        message,
    }
}

pub fn mount_disk(device: &str) -> MountResult {
    match run("udisksctl", &["mount", "-b", device]) {
        Ok(out) => match parse_udisks_mount(&out) {
            Some(path) => done(path, "Mounted"),
            None => failed(format!(
                "mounted, but no mount point reported: {}",
                out.trim()
            )),
        },
        // udisks refuses when it is already mounted; the caller refreshes and navigates.
        Err(e) => failed(e),
    }
}

/// Unmount a disk and, for removable media, power the drive off.
pub fn eject_disk(device: &str, removable: bool) -> MountResult {
    if let Err(e) = run("udisksctl", &["unmount", "-b", device])
        && !e.contains("not mounted")
    {
        return failed(e);
    }
    if removable {
        let parent = device
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .trim_end_matches('p');
        let target = if parent.len() < "/dev/".len() {
            device
        } else {
            parent
        };
        if let Err(e) = run("udisksctl", &["power-off", "-b", target]) {
            return failed(e);
        }
        return done(String::new(), "Ejected");
    }
    done(String::new(), "Unmounted")
}

pub fn unmount_remote(uri_or_path: &str) -> MountResult {
    match run("gio", &["mount", "-u", "--", uri_or_path]) {
        Ok(_) => done(String::new(), "Unmounted"),
        Err(e) => failed(e),
    }
}

/// Answers to `gio mount` prompts, delivered from the UI thread.
static ANSWERS: Mutex<Option<Sender<Option<String>>>> = Mutex::new(None);

/// Deliver the answer (or `None` to cancel) to the prompt `mount_uri` is waiting on.
pub fn answer_prompt(answer: Option<String>) {
    if let Some(tx) = ANSWERS.lock().unwrap().as_ref() {
        let _ = tx.send(answer);
    }
}

/// Which credential a `gio mount` prompt asks for, if any.
pub fn prompt_kind(prompt: &str) -> Option<&'static str> {
    let p = prompt.to_lowercase();
    if p.contains("password") {
        Some("password")
    } else if p.contains("user") {
        Some("user")
    } else if p.contains("domain") {
        Some("domain")
    } else if p.trim_end().ends_with("[y/n]:") || p.contains("(yes/no") {
        Some("confirm")
    } else {
        None
    }
}

/// Mount `uri` with `gio mount`, relaying credential prompts through `ask`.
/// `ask` gets the prompt text and returns the answer, or `None` to cancel.
pub fn mount_uri(uri: &str, ask: impl Fn(&str) -> Option<String>) -> MountResult {
    let Ok(mut child) = Command::new("gio")
        .args(["mount", "--", uri])
        .env("LC_ALL", "C")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return failed("gio is not installed".into());
    };
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();

    let (tx, rx): (Sender<Vec<u8>>, Receiver<Vec<u8>>) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 256];
        while let Ok(n) = stdout.read(&mut buf) {
            if n == 0 || tx.send(buf[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });

    let mut pending = String::new();
    loop {
        match rx.recv_timeout(Duration::from_millis(400)) {
            Ok(bytes) => pending.push_str(&String::from_utf8_lossy(&bytes)),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // A prompt is output without a trailing newline and then gio waits for input.
                let prompt = pending
                    .trim_end_matches('\n')
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .to_owned();
                if prompt_kind(&prompt).is_some() && !prompt.is_empty() {
                    pending.clear();
                    match ask(&prompt) {
                        Some(answer) => {
                            let _ = writeln!(stdin, "{answer}");
                            let _ = stdin.flush();
                        }
                        None => {
                            let _ = child.kill();
                            let _ = child.wait();
                            return failed("Cancelled".into());
                        }
                    }
                }
                if matches!(child.try_wait(), Ok(Some(_))) {
                    break;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let status = child.wait();
    let err = err_reader.join().unwrap_or_default();
    match status {
        Ok(s) if s.success() => done(find_gvfs_path(uri).unwrap_or_default(), "Connected"),
        _ => {
            let msg = err
                .trim()
                .lines()
                .last()
                .unwrap_or("could not connect")
                .to_owned();
            // Already connected is the state the caller wanted: open it.
            if msg.to_lowercase().contains("already mounted") {
                return match find_gvfs_path(uri) {
                    Some(path) => done(path, "Already connected"),
                    None => failed("Already mounted by gvfs but not browsable; unmount it from the sidebar and retry".into()),
                };
            }
            failed(msg)
        }
    }
}

/// Register a channel the UI answers prompts through; returns the receiving end.
pub fn begin_prompting() -> Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    *ANSWERS.lock().unwrap() = Some(tx);
    rx
}

/// The service (as GVfs names it), host and share of a URI.
fn uri_parts(uri: &str) -> Option<(&str, &str, &str)> {
    let (scheme, rest) = uri.split_once("://")?;
    let gvfs_scheme = match scheme {
        "smb" => "smb-share",
        "http" | "dav" => "dav",
        "https" | "davs" => "davs",
        "afp" => "afp-volume",
        "ssh" => "sftp",
        s => s,
    };
    let host = rest.split('/').next()?.rsplit('@').next()?;
    let host = host.split(':').next()?;
    Some((gvfs_scheme, host, rest.split('/').nth(1).unwrap_or("")))
}

/// Does the GVfs entry `name` (`smb-share:server=...,share=...`) have this service and share?
fn entry_has_share(name: &str, scheme: &str, share: &str) -> bool {
    let (entry_scheme, params) = split_gvfs_name(name);
    entry_scheme == scheme
        && (share.is_empty()
            || !name.starts_with("smb-share")
            || params
                .iter()
                .any(|(k, v)| k == "share" && v.eq_ignore_ascii_case(share)))
}

/// Index of a FUSE entry no other mount owns that has this URI's service and share, whatever host it
/// goes by: the folder-less gio mount and the entry are the same mount.
fn twin_entry(entries: &[String], claimed: &[String], uri: &str) -> Option<usize> {
    let (scheme, _, share) = uri_parts(uri)?;
    entries
        .iter()
        .position(|n| !claimed.contains(n) && entry_has_share(n, scheme, share))
}

/// The GVfs FUSE entry (directory name and path) for a mounted URI.
pub fn find_gvfs_entry(uri: &str) -> Option<(String, String)> {
    let (scheme, host, share) = uri_parts(uri)?;
    for e in fs::read_dir(gvfs_dir()).ok()?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let (_, params) = split_gvfs_name(&name);
        let has_host = params
            .iter()
            .any(|(k, v)| (k == "host" || k == "server") && v.eq_ignore_ascii_case(host));
        if has_host && entry_has_share(&name, scheme, share) {
            return Some((name, e.path().to_string_lossy().into_owned()));
        }
    }
    None
}

/// Find the GVfs FUSE directory for a freshly mounted URI.
pub fn find_gvfs_path(uri: &str) -> Option<String> {
    find_gvfs_entry(uri).map(|e| e.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LSBLK: &str = r#"{"blockdevices":[
      {"name":"zram0","path":"/dev/zram0","label":"zram0","size":"31G","type":"disk","mountpoints":["[SWAP]"],"fstype":"swap","rm":false,"hotplug":false,"model":null},
      {"name":"sda","path":"/dev/sda","label":null,"size":"58G","type":"disk","mountpoints":[null],"fstype":null,"rm":true,"hotplug":true,"model":"USB DISK",
       "children":[{"name":"sda1","path":"/dev/sda1","label":"STICK","size":"58G","type":"part","mountpoints":["/run/media/u/STICK"],"fstype":"exfat","rm":true,"hotplug":true,"model":null},
                   {"name":"sda2","path":"/dev/sda2","label":null,"size":"1G","type":"part","mountpoints":[null],"fstype":"ext4","rm":true,"hotplug":true,"model":null},
                   {"name":"sda3","path":"/dev/sda3","label":null,"size":"1G","type":"part","mountpoints":[null],"fstype":"crypto_LUKS","rm":true,"hotplug":true,"model":null}]}]}"#;

    #[test]
    fn lsblk_lists_filesystems_only() {
        let d = parse_lsblk(LSBLK);
        let ids: Vec<_> = d.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["disk:/dev/sda1", "disk:/dev/sda2"]);
        assert!(d[0].mounted && d[0].path == "/run/media/u/STICK" && d[0].label == "STICK");
        assert!(!d[1].mounted && d[1].removable);
    }

    #[test]
    fn gvfs_names_become_uris() {
        assert_eq!(
            gvfs_label("smb-share:server=nas,share=media"),
            "smb://nas/media"
        );
        assert_eq!(
            gvfs_label("sftp:host=example.com,user=bob"),
            "sftp://bob@example.com"
        );
        assert_eq!(gvfs_kind("mtp:host=Pixel_7"), "mtp");
        assert_eq!(gvfs_kind("sftp:host=x"), "remote");
    }

    #[test]
    fn gio_volume_parsing() {
        let text = "Drive(0): Pixel\n  Type: GProxyDrive\nVolume(0): Pixel 7\n  Type: GProxyVolume\n  activation_root=mtp://Google_Pixel_7/\n  can_mount=1\nVolume(1): Data\n  activation_root=file:///mnt\n";
        assert_eq!(
            parse_gio_volumes(text),
            [("Pixel 7".to_owned(), "mtp://Google_Pixel_7/".to_owned())]
        );
    }

    #[test]
    fn udisks_output() {
        assert_eq!(
            parse_udisks_mount("Mounted /dev/sdb1 at /run/media/u/DISK.\n").as_deref(),
            Some("/run/media/u/DISK")
        );
        assert_eq!(parse_udisks_mount("nope"), None);
    }

    #[test]
    fn prompts_are_recognised() {
        assert_eq!(prompt_kind("Password: "), Some("password"));
        assert_eq!(prompt_kind("User [bob]: "), Some("user"));
        assert_eq!(prompt_kind("Domain [WORKGROUP]: "), Some("domain"));
        assert_eq!(prompt_kind("Mounting..."), None);
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;

    #[test]
    fn space_reports_used_within_total() {
        let (used, total) = space("/").unwrap();
        assert!(total > 0 && used <= total, "{used} of {total}");
        assert!(space("/definitely/not/here").is_none());
    }

    #[test]
    fn usage_is_a_percentage_or_unknown() {
        let u = usage_percent("/");
        assert!((0..=100).contains(&u), "{u}");
        assert_eq!(usage_percent("/definitely/not/here"), -1);
        assert_eq!(usage_percent("bad\0path"), -1);
    }

    #[test]
    fn btrfs_style_multiple_mounts_pick_the_shortest() {
        let json = r#"{"blockdevices":[{"name":"nvme0n1p2","path":"/dev/nvme0n1p2","label":"root","size":"236G","type":"part",
            "mountpoints":["/var/cache/pacman/pkg","/","/home"],"fstype":"btrfs","rm":false,"hotplug":false}]}"#;
        let d = parse_lsblk(json);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "/");
        assert_eq!(d[0].detail, "236G btrfs");
    }

    #[test]
    fn garbage_lsblk_output_gives_no_devices() {
        assert!(parse_lsblk("not json").is_empty());
        assert!(parse_lsblk("{}").is_empty());
    }

    #[test]
    fn a_mount_under_another_host_spelling_is_one_row() {
        let entries = vec![
            "smb-share:server=nas,share=s00".to_owned(),
            "smb-share:server=nas,share=s01".to_owned(),
        ];
        // gio says "192.168.1.250" for the share the FUSE entry calls "nas".
        assert_eq!(
            twin_entry(&entries, &[], "smb://192.168.1.250/s01/"),
            Some(1)
        );
        // An entry another mount already owns is not handed out twice.
        assert_eq!(
            twin_entry(&entries, &[entries[1].clone()], "smb://192.168.1.250/s01/"),
            None
        );
        assert_eq!(twin_entry(&entries, &[], "sftp://192.168.1.250/"), None);
    }

    #[test]
    fn gio_mounts_skip_local_and_keep_remote() {
        let text = "Drive(0): X\n  Type: GProxyDrive\n  Volume(0): USB\n    Mount(0): USB -> file:///run/media/u/USB\nMount(0): media on nas -> smb://nas/media\n";
        assert_eq!(
            parse_gio_mounts(text),
            vec![("media on nas".to_owned(), "smb://nas/media".to_owned())]
        );
    }

    #[test]
    fn gvfs_labels_for_other_schemes() {
        assert_eq!(
            gvfs_label("dav:host=cloud.example,ssl=true"),
            "http://cloud.example"
        );
        assert_eq!(
            gvfs_label("davs:host=cloud.example"),
            "https://cloud.example"
        );
        assert_eq!(gvfs_label("ftp:host=files.example"), "ftp://files.example");
    }

    #[test]
    fn gio_info_reports_used_and_total() {
        let out = "uri: smb://x/y/\nlocal path: /run/user/1000/gvfs/x\n  filesystem::size: 8127826726912\n  filesystem::free: 4162777456640\n  filesystem::type: cifs\n";
        assert_eq!(
            parse_gio_space(out),
            Some((8127826726912 - 4162777456640, 8127826726912))
        );
        assert_eq!(parse_gio_space("  filesystem::size: 100\n"), None);
        assert_eq!(parse_gio_space("  filesystem::free: 5\n"), None);
        assert_eq!(
            parse_gio_space("  filesystem::size: 0\n  filesystem::free: 0\n"),
            None
        );
        // Free larger than size (odd servers) must not wrap around.
        assert_eq!(
            parse_gio_space("  filesystem::size: 10\n  filesystem::free: 12\n"),
            Some((0, 10))
        );
    }

    #[test]
    fn space_answers_stay_fresh_for_a_minute() {
        let mut cache = SpaceCache::default();
        let t0 = std::time::Instant::now();
        assert!(cache.get("/a", t0).is_none());
        cache.put("/a", t0, Some((1, 2)));
        cache.put("/dead", t0, None);
        assert_eq!(
            cache.get("/a", t0 + std::time::Duration::from_secs(59)),
            Some(Some((1, 2)))
        );
        assert_eq!(
            cache.get("/dead", t0 + std::time::Duration::from_secs(1)),
            Some(None)
        );
        assert!(
            cache
                .get("/a", t0 + std::time::Duration::from_secs(61))
                .is_none()
        );
    }

    #[test]
    fn a_slow_program_is_given_up_on() {
        let t = std::time::Instant::now();
        assert!(run_limited("sleep", &["5"], std::time::Duration::from_millis(200)).is_none());
        assert!(t.elapsed() < std::time::Duration::from_secs(2));
        assert_eq!(
            run_limited("echo", &["hi"], std::time::Duration::from_secs(2)).as_deref(),
            Some("hi\n")
        );
    }
}
