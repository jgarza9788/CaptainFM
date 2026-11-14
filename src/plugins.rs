//! Plugins: `~/.config/captainfm/plugins/<name>/plugin.json` can add QML panels and Lua actions.
//!
//! ```json
//! { "name": "Word count",
//!   "panels":  [{ "id": "stats", "title": "Stats", "qml": "Stats.qml", "side": "right" }],
//!   "actions": [{ "id": "count", "title": "Count lines", "keys": ["<Space> p c"],
//!                 "file": "main.lua", "function": "count" }] }
//! ```
//!
//! A plugin can also provide previews: `"previews": [{ "id": "torrent", "ext": ["torrent"], "function": "preview" }]`
//! (`magic` matches the first bytes as hex instead of the extension). The function gets
//! `{ path, name, ext, size, mtime }` and returns `{ text, note }`, `{ image = "/path/to.png", note }`, or nothing to
//! let the next plugin or the built-in preview try. The rarely used previews ship inside the app this way
//! (`packaging/plugins/`) and can be switched off in Settings; a plugin of the same name in the config folder replaces one.
//!
//! Lua actions run in a small sandbox (string/table/math/utf8 only) with a `cfm` table for the
//! file manager side effects; they return the "effects" for the UI to apply.

use crate::lock::MutexExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mlua::{HookTriggers, Lua, LuaOptions, LuaSerdeExt, StdLib, Value as LuaValue, VmState};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::state::config_dir;

const ACTION_TIME_LIMIT: Duration = Duration::from_secs(10);
const PROCESS_TIME_LIMIT: Duration = Duration::from_secs(30);

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
struct PanelSpec {
    id: String,
    title: String,
    qml: String,
    /// right | bottom
    side: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
struct ActionSpec {
    id: String,
    title: String,
    keys: Vec<String>,
    /// Lua file inside the plugin folder (default `main.lua`).
    file: String,
    /// Global function to call (default: the action id).
    function: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
struct PreviewSpec {
    id: String,
    /// File name endings, without the dot (`torrent`, `tar.gz`), lower case.
    ext: Vec<String>,
    /// First bytes of the file as hex (`7f454c46`), for files that have no telling extension.
    magic: Vec<String>,
    file: String,
    /// Global function to call (default `preview`).
    function: String,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(default)]
struct Manifest {
    name: String,
    panels: Vec<PanelSpec>,
    actions: Vec<ActionSpec>,
    previews: Vec<PreviewSpec>,
}

/// Plugins that ship inside the app: id, `plugin.json`, then their scripts.
type Bundled = (
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const BUNDLED: &[Bundled] = &[
    #[cfg(feature = "plugin-torrent")]
    (
        "preview-torrent",
        include_str!("../packaging/plugins/preview-torrent/plugin.json"),
        &[(
            "main.lua",
            include_str!("../packaging/plugins/preview-torrent/main.lua"),
        )],
    ),
    #[cfg(feature = "plugin-calendar-contacts")]
    (
        "preview-calendar-contacts",
        include_str!("../packaging/plugins/preview-calendar-contacts/plugin.json"),
        &[(
            "main.lua",
            include_str!("../packaging/plugins/preview-calendar-contacts/main.lua"),
        )],
    ),
    #[cfg(feature = "plugin-sqlite")]
    (
        "preview-sqlite",
        include_str!("../packaging/plugins/preview-sqlite/plugin.json"),
        &[(
            "main.lua",
            include_str!("../packaging/plugins/preview-sqlite/main.lua"),
        )],
    ),
    #[cfg(feature = "plugin-system")]
    (
        "preview-system",
        include_str!("../packaging/plugins/preview-system/plugin.json"),
        &[(
            "main.lua",
            include_str!("../packaging/plugins/preview-system/main.lua"),
        )],
    ),
    #[cfg(feature = "plugin-documents")]
    (
        "preview-documents",
        include_str!("../packaging/plugins/preview-documents/plugin.json"),
        &[(
            "main.lua",
            include_str!("../packaging/plugins/preview-documents/main.lua"),
        )],
    ),
];

/// An installed plugin: from the config folder, or one of the bundled ones.
struct Plugin {
    id: String,
    /// Folder of a plugin in the config folder; `None` for a bundled one.
    dir: Option<PathBuf>,
    manifest: Manifest,
    enabled: bool,
}

impl Plugin {
    fn script(&self, file: &str) -> Result<String, String> {
        match &self.dir {
            Some(dir) => {
                std::fs::read_to_string(dir.join(file)).map_err(|e| format!("{file}: {e}"))
            }
            None => BUNDLED
                .iter()
                .find(|b| b.0 == self.id)
                .and_then(|b| b.2.iter().find(|f| f.0 == file))
                .map(|f| f.1.to_owned())
                .ok_or_else(|| format!("{file}: not found")),
        }
    }
}

/// Plugin ids switched off in Settings (`disabledPlugins` in `settings.json`).
fn disabled_plugins() -> Vec<String> {
    serde_json::from_str::<Value>(&crate::state::read_config("settings"))
        .ok()
        .and_then(|v| v.get("disabledPlugins").cloned())
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Turn a plugin on or off in `settings.json` (what Settings > Plugins does), for `captainfm --plugin`.
/// `name` is a plugin id, or the part after `preview-` (`torrent`). `all` switches every plugin.
pub fn set_enabled(name: &str, on: bool) -> Result<String, String> {
    let known: Vec<String> = all_plugins().into_iter().map(|p| p.id).collect();
    let targets: Vec<String> = if name == "all" {
        known.clone()
    } else {
        let id = [name.to_owned(), format!("preview-{name}")]
            .into_iter()
            .find(|c| known.contains(c));
        vec![id.ok_or_else(|| format!("no plugin “{name}” (have: {})", known.join(", ")))?]
    };
    let mut settings: Value =
        serde_json::from_str(&crate::state::read_config("settings")).unwrap_or_else(|_| json!({}));
    let mut off = disabled_plugins();
    off.retain(|id| !targets.contains(id));
    if !on {
        off.extend(targets.iter().cloned());
    }
    if let Some(map) = settings.as_object_mut() {
        if off.is_empty() {
            map.remove("disabledPlugins");
        } else {
            map.insert("disabledPlugins".into(), json!(off));
        }
    }
    if !crate::state::write_config("settings", &settings.to_string()) {
        return Err("could not write settings.json".into());
    }
    Ok(format!(
        "{} {}",
        targets.join(", "),
        if on { "on" } else { "off" }
    ))
}

/// One line per plugin for `captainfm --plugins`.
pub fn describe() -> String {
    if all_plugins().is_empty() {
        return "no plugins".to_owned();
    }
    all_plugins()
        .iter()
        .map(|p| {
            let what: Vec<String> = p
                .manifest
                .previews
                .iter()
                .flat_map(|s| s.ext.clone())
                .collect();
            format!(
                "{:<28} {:<4} {}{}",
                p.id,
                if p.enabled { "on" } else { "off" },
                if p.dir.is_none() { "built in" } else { "yours" },
                if what.is_empty() {
                    String::new()
                } else {
                    format!(" · previews {}", what.join(", "))
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every plugin, by id: those in the config folder, then the bundled ones that it does not replace.
fn all_plugins() -> Vec<Plugin> {
    let off = disabled_plugins();
    let mut out: Vec<Plugin> = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(plugins_dir())
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    for dir in dirs {
        let Some(manifest) = read_manifest(&dir) else {
            continue;
        };
        let Some(id) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let enabled = !off.contains(&id);
        out.push(Plugin {
            id,
            dir: Some(dir),
            manifest,
            enabled,
        });
    }
    for (id, json, _) in BUNDLED {
        if out.iter().any(|p| p.id == *id) {
            continue;
        }
        if let Ok(manifest) = serde_json::from_str::<Manifest>(json) {
            out.push(Plugin {
                id: (*id).to_owned(),
                dir: None,
                manifest,
                enabled: !off.contains(&(*id).to_owned()),
            });
        }
    }
    out
}

fn plugins_dir() -> PathBuf {
    config_dir().join("plugins")
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let text = std::fs::read_to_string(dir.join("plugin.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Describe every installed plugin as JSON for the UI.
pub fn list() -> Value {
    let mut out = Vec::new();
    for plugin in all_plugins() {
        let Plugin {
            id,
            dir,
            manifest,
            enabled,
        } = plugin;
        let dir_path = dir.clone().unwrap_or_default();
        out.push(json!({
            "id": id,
            "name": if manifest.name.is_empty() { id.clone() } else { manifest.name.clone() },
            "bundled": dir.is_none(),
            "enabled": enabled,
            "previews": manifest.previews.iter().flat_map(|p| p.ext.clone()).collect::<Vec<_>>(),
            "panels": manifest.panels.iter().map(|p| json!({
                "id": p.id,
                "title": if p.title.is_empty() { p.id.clone() } else { p.title.clone() },
                "qml": format!("file://{}", dir_path.join(&p.qml).to_string_lossy()),
                "side": if p.side == "bottom" { "bottom" } else { "right" },
            })).collect::<Vec<_>>(),
            "actions": manifest.actions.iter().map(|a| json!({
                "id": a.id,
                "title": if a.title.is_empty() { a.id.clone() } else { a.title.clone() },
                "keys": a.keys,
            })).collect::<Vec<_>>(),
        }));
    }
    Value::Array(out)
}

type Effects = Arc<Mutex<Vec<Value>>>;

fn push(effects: &Effects, effect: Value) {
    effects.locked().push(effect);
}

/// Run `command` with a time limit, capturing output.
fn run_process(command: &str, args: &[String], cwd: Option<&str>) -> Value {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return json!({"ok": false, "code": -1, "stdout": "", "stderr": e.to_string()}),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > PROCESS_TIME_LIMIT => {
                let _ = child.kill();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => break,
        }
    }
    match child.wait_with_output() {
        Ok(o) => json!({
            "ok": o.status.success(),
            "code": o.status.code().unwrap_or(-1),
            "stdout": String::from_utf8_lossy(&o.stdout),
            "stderr": String::from_utf8_lossy(&o.stderr),
        }),
        Err(e) => json!({"ok": false, "code": -1, "stdout": "", "stderr": e.to_string()}),
    }
}

fn install_api(lua: &Lua, effects: &Effects, cwd: String) -> mlua::Result<()> {
    let cfm = lua.create_table()?;

    let e = Arc::clone(effects);
    cfm.set(
        "notify",
        lua.create_function(move |_, (message, kind): (String, Option<String>)| {
            push(&e, json!({"type": "toast", "message": message, "kind": kind.unwrap_or_else(|| "info".into())}));
            Ok(())
        })?,
    )?;
    let e = Arc::clone(effects);
    cfm.set(
        "navigate",
        lua.create_function(move |_, path: String| {
            push(&e, json!({"type": "navigate", "path": path}));
            Ok(())
        })?,
    )?;
    let e = Arc::clone(effects);
    cfm.set(
        "select",
        lua.create_function(move |_, path: String| {
            push(&e, json!({"type": "select", "path": path}));
            Ok(())
        })?,
    )?;
    let e = Arc::clone(effects);
    cfm.set(
        "open",
        lua.create_function(move |_, path: String| {
            push(&e, json!({"type": "open", "path": path}));
            Ok(())
        })?,
    )?;
    let e = Arc::clone(effects);
    cfm.set(
        "copy_text",
        lua.create_function(move |_, text: String| {
            push(&e, json!({"type": "copy", "text": text}));
            Ok(())
        })?,
    )?;
    let e = Arc::clone(effects);
    cfm.set(
        "command",
        lua.create_function(move |_, id: String| {
            push(&e, json!({"type": "command", "id": id}));
            Ok(())
        })?,
    )?;
    cfm.set(
        "log",
        lua.create_function(|_, message: String| {
            eprintln!("[captainfm plugin] {message}");
            Ok(())
        })?,
    )?;

    // Process and filesystem helpers (read-only: plugins change files through `cfm.run`).
    let default_cwd = cwd.clone();
    cfm.set(
        "run",
        lua.create_function(
            move |lua, (command, args, dir): (String, Option<Vec<String>>, Option<String>)| {
                let result = run_process(
                    &command,
                    &args.unwrap_or_default(),
                    Some(&dir.unwrap_or_else(|| default_cwd.clone())),
                );
                lua.to_value(&result)
            },
        )?,
    )?;
    cfm.set(
        "read",
        lua.create_function(|_, (path, max): (String, Option<usize>)| {
            use std::io::Read;
            let mut buf = Vec::new();
            let limit = max.unwrap_or(1 << 20) as u64;
            Ok(std::fs::File::open(path)
                .and_then(|f| f.take(limit).read_to_end(&mut buf))
                .ok()
                .map(|_| String::from_utf8_lossy(&buf).into_owned()))
        })?,
    )?;
    // The file's bytes exactly as they are (`read` turns invalid UTF-8 into replacement characters).
    cfm.set(
        "read_bytes",
        lua.create_function(|lua, (path, max): (String, Option<usize>)| {
            use std::io::Read;
            let mut buf = Vec::new();
            let limit = max.unwrap_or(1 << 20) as u64;
            match std::fs::File::open(path).and_then(|f| f.take(limit).read_to_end(&mut buf)) {
                Ok(_) => Ok(Some(lua.create_string(&buf)?)),
                Err(_) => Ok(None),
            }
        })?,
    )?;
    // A file name in the preview cache for `key` (anything that identifies the result, like path and date),
    // with extension `ext`. The folder exists; the file is the plugin's to create with `cfm.run`.
    cfm.set(
        "cache_file",
        lua.create_function(|_, (key, ext): (String, Option<String>)| {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            key.hash(&mut h);
            let dir = crate::state::cache_dir().join("preview");
            let _ = std::fs::create_dir_all(&dir);
            let ext: String = ext
                .unwrap_or_default()
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect();
            Ok(dir
                .join(format!(
                    "plugin-{:x}.{}",
                    h.finish(),
                    if ext.is_empty() { "dat" } else { &ext }
                ))
                .to_string_lossy()
                .into_owned())
        })?,
    )?;
    cfm.set(
        "exists",
        lua.create_function(|_, path: String| Ok(std::fs::symlink_metadata(path).is_ok()))?,
    )?;
    cfm.set(
        "is_dir",
        lua.create_function(|_, path: String| Ok(Path::new(&path).is_dir()))?,
    )?;
    cfm.set(
        "list",
        lua.create_function(|_, path: String| {
            let names: Vec<String> = std::fs::read_dir(path)
                .map(|rd| {
                    rd.flatten()
                        .take(5000)
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            Ok(names)
        })?,
    )?;
    cfm.set(
        "env",
        lua.create_function(|_, name: String| Ok(std::env::var(name).ok()))?,
    )?;
    cfm.set(
        "home",
        lua.create_function(|_, ()| Ok(crate::state::home().to_string_lossy().into_owned()))?,
    )?;

    lua.globals().set("cfm", cfm)?;
    Ok(())
}

/// A sandboxed Lua state with the `cfm` API and a time limit.
fn make_lua(limit: Duration, effects: &Effects, cwd: String) -> Result<Lua, String> {
    let lua = Lua::new_with(
        StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
        LuaOptions::default(),
    )
    .map_err(|e| e.to_string())?;
    let started = Instant::now();
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(100_000),
        move |_, _| {
            if started.elapsed() > limit {
                Err(mlua::Error::runtime("plugin action took too long"))
            } else {
                Ok(VmState::Continue)
            }
        },
    )
    .map_err(|e| e.to_string())?;
    install_api(&lua, effects, cwd).map_err(|e| e.to_string())?;
    Ok(lua)
}

fn run_lua(
    script: &str,
    chunk_name: &str,
    function: &str,
    ctx: &Value,
    limit: Duration,
) -> Result<Vec<Value>, String> {
    let effects: Effects = Arc::new(Mutex::new(Vec::new()));
    let cwd = ctx
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("/")
        .to_owned();
    let lua = make_lua(limit, &effects, cwd)?;

    lua.load(script)
        .set_name(chunk_name)
        .exec()
        .map_err(|e| e.to_string())?;
    let func: mlua::Function = lua
        .globals()
        .get(function)
        .map_err(|_| format!("{chunk_name} defines no function `{function}`"))?;
    let arg = lua.to_value(ctx).map_err(|e| e.to_string())?;
    let returned: LuaValue = func.call(arg).map_err(|e| e.to_string())?;

    let mut out = std::mem::take(&mut *effects.locked());
    if !matches!(returned, LuaValue::Nil)
        && let Ok(v) = lua.from_value::<Value>(returned)
    {
        out.push(json!({"type": "data", "value": v}));
    }
    Ok(out)
}

/// What a plugin preview produced.
pub enum Preview {
    Text { text: String, note: String },
    Image { path: String, note: String },
}

fn matches_ext(name: &str, exts: &[String]) -> bool {
    let name = name.to_lowercase();
    exts.iter()
        .any(|e| name.len() > e.len() + 1 && name.ends_with(&format!(".{}", e.to_lowercase())))
}

/// Ask the enabled plugins for a preview of `path`; the first one that returns something wins.
pub fn preview(path: &str, name: &str, ext: &str, size: i64, mtime: i64) -> Option<Preview> {
    let plugins: Vec<Plugin> = all_plugins()
        .into_iter()
        .filter(|p| p.enabled && !p.manifest.previews.is_empty())
        .collect();
    if plugins.is_empty() {
        return None;
    }
    // The first bytes, read only when some plugin matches on them.
    let head: Option<String> = plugins
        .iter()
        .any(|p| p.manifest.previews.iter().any(|s| !s.magic.is_empty()))
        .then(|| {
            use std::io::Read;
            let mut buf = [0u8; 16];
            let n = std::fs::File::open(path)
                .and_then(|mut f| f.read(&mut buf))
                .unwrap_or(0);
            buf[..n].iter().map(|b| format!("{b:02x}")).collect()
        });
    let ctx = json!({"path": path, "name": name, "ext": ext, "size": size, "mtime": mtime});
    for plugin in &plugins {
        for spec in &plugin.manifest.previews {
            let by_magic = head
                .as_ref()
                .is_some_and(|h| spec.magic.iter().any(|m| h.starts_with(&m.to_lowercase())));
            if !by_magic && !matches_ext(name, &spec.ext) {
                continue;
            }
            let file = if spec.file.is_empty() {
                "main.lua"
            } else {
                spec.file.as_str()
            };
            let function = if spec.function.is_empty() {
                "preview"
            } else {
                spec.function.as_str()
            };
            let Ok(script) = plugin.script(file) else {
                continue;
            };
            match run_preview(&script, &format!("{}/{file}", plugin.id), function, &ctx) {
                Ok(Some(found)) => {
                    crate::state::FeatureUse::mark(&format!("plugin:{}", plugin.id));
                    return Some(found);
                }
                Ok(None) => {}
                Err(e) => eprintln!("[captainfm plugin] {}: {e}", plugin.id),
            }
        }
    }
    None
}

fn run_preview(
    script: &str,
    chunk_name: &str,
    function: &str,
    ctx: &Value,
) -> Result<Option<Preview>, String> {
    let effects: Effects = Arc::new(Mutex::new(Vec::new()));
    let lua = make_lua(ACTION_TIME_LIMIT, &effects, "/".to_owned())?;
    lua.load(script)
        .set_name(chunk_name)
        .exec()
        .map_err(|e| e.to_string())?;
    let func: mlua::Function = lua
        .globals()
        .get(function)
        .map_err(|_| format!("{chunk_name} defines no function `{function}`"))?;
    let arg = lua.to_value(ctx).map_err(|e| e.to_string())?;
    let returned: LuaValue = func.call(arg).map_err(|e| e.to_string())?;
    let LuaValue::Table(t) = returned else {
        return Ok(None);
    };
    // Text may hold bytes that are not UTF-8; take them as they are and replace the bad ones.
    let get = |key: &str| -> Option<String> {
        t.get::<Option<mlua::LuaString>>(key)
            .ok()
            .flatten()
            .map(|s| String::from_utf8_lossy(&s.as_bytes()).into_owned())
    };
    let note = get("note").unwrap_or_default();
    if let Some(path) = get("image") {
        return Ok(Some(Preview::Image { path, note }));
    }
    Ok(get("text")
        .filter(|t| !t.is_empty())
        .map(|text| Preview::Text { text, note }))
}

/// Run one plugin action; the result is a JSON array of effects (or a single error effect).
pub fn run_action(plugin: &str, action: &str, ctx: &Value) -> Value {
    let safe = |s: &str| !s.is_empty() && !s.contains('/') && !s.contains("..");
    if !safe(plugin) {
        return json!([{"type": "toast", "message": "invalid plugin name", "kind": "error"}]);
    }
    let Some(found) = all_plugins().into_iter().find(|p| p.id == plugin) else {
        return json!([{"type": "toast", "message": format!("plugin “{plugin}” not found"), "kind": "error"}]);
    };
    let Some(spec) = found.manifest.actions.iter().find(|a| a.id == action) else {
        return json!([{"type": "toast", "message": format!("{plugin} has no action “{action}”"), "kind": "error"}]);
    };
    let file = if spec.file.is_empty() {
        "main.lua"
    } else {
        spec.file.as_str()
    };
    let function = if spec.function.is_empty() {
        spec.id.as_str()
    } else {
        spec.function.as_str()
    };
    if !safe(file) {
        return json!([{"type": "toast", "message": "invalid script name", "kind": "error"}]);
    }
    let script = match found.script(file) {
        Ok(s) => s,
        Err(e) => return json!([{"type": "toast", "message": e, "kind": "error"}]),
    };
    match run_lua(
        &script,
        &format!("{plugin}/{file}"),
        function,
        ctx,
        ACTION_TIME_LIMIT,
    ) {
        Ok(effects) => Value::Array(effects),
        Err(e) => {
            let msg = e.lines().next().unwrap_or("error").to_owned();
            json!([{"type": "toast", "message": format!("{plugin}: {msg}"), "kind": "error"}])
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod install_tests;
