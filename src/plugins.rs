//! Plugins: `~/.config/captainfm/plugins/<name>/plugin.json` can add QML panels and Lua actions.
//!
//! ```json
//! { "name": "Word count",
//!   "panels":  [{ "id": "stats", "title": "Stats", "qml": "Stats.qml", "side": "right" }],
//!   "actions": [{ "id": "count", "title": "Count lines", "keys": ["<Space> p c"],
//!                 "file": "main.lua", "function": "count" }] }
//! ```
//!
//! Lua actions run in a small sandbox (string/table/math/utf8 only) with a `cfm` table for the
//! file manager side effects; they return the "effects" for the UI to apply.

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
struct Manifest {
    name: String,
    panels: Vec<PanelSpec>,
    actions: Vec<ActionSpec>,
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
    let Ok(rd) = std::fs::read_dir(plugins_dir()) else {
        return json!([]);
    };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for dir in dirs {
        let Some(manifest) = read_manifest(&dir) else { continue };
        let id = dir.file_name().unwrap().to_string_lossy().into_owned();
        out.push(json!({
            "id": id,
            "name": if manifest.name.is_empty() { id.clone() } else { manifest.name.clone() },
            "panels": manifest.panels.iter().map(|p| json!({
                "id": p.id,
                "title": if p.title.is_empty() { p.id.clone() } else { p.title.clone() },
                "qml": format!("file://{}", dir.join(&p.qml).to_string_lossy()),
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
    effects.lock().unwrap().push(effect);
}

/// Run `command` with a time limit, capturing output.
fn run_process(command: &str, args: &[String], cwd: Option<&str>) -> Value {
    let mut cmd = Command::new(command);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
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
        lua.create_function(move |lua, (command, args, dir): (String, Option<Vec<String>>, Option<String>)| {
            let result = run_process(&command, &args.unwrap_or_default(), Some(&dir.unwrap_or_else(|| default_cwd.clone())));
            lua.to_value(&result)
        })?,
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
    cfm.set("exists", lua.create_function(|_, path: String| Ok(std::fs::symlink_metadata(path).is_ok()))?)?;
    cfm.set("is_dir", lua.create_function(|_, path: String| Ok(Path::new(&path).is_dir()))?)?;
    cfm.set(
        "list",
        lua.create_function(|_, path: String| {
            let names: Vec<String> = std::fs::read_dir(path)
                .map(|rd| rd.flatten().take(5000).map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default();
            Ok(names)
        })?,
    )?;
    cfm.set("env", lua.create_function(|_, name: String| Ok(std::env::var(name).ok()))?)?;
    cfm.set("home", lua.create_function(|_, ()| Ok(crate::state::home().to_string_lossy().into_owned()))?)?;

    lua.globals().set("cfm", cfm)?;
    Ok(())
}

fn run_lua(
    script: &str,
    chunk_name: &str,
    function: &str,
    ctx: &Value,
    limit: Duration,
) -> Result<Vec<Value>, String> {
    let lua = Lua::new_with(StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8, LuaOptions::default())
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    lua.set_hook(HookTriggers::new().every_nth_instruction(100_000), move |_, _| {
        if started.elapsed() > limit {
            Err(mlua::Error::runtime("plugin action took too long"))
        } else {
            Ok(VmState::Continue)
        }
    })
    .map_err(|e| e.to_string())?;

    let effects: Effects = Arc::new(Mutex::new(Vec::new()));
    let cwd = ctx.get("path").and_then(Value::as_str).unwrap_or("/").to_owned();
    install_api(&lua, &effects, cwd).map_err(|e| e.to_string())?;

    lua.load(script).set_name(chunk_name).exec().map_err(|e| e.to_string())?;
    let func: mlua::Function = lua
        .globals()
        .get(function)
        .map_err(|_| format!("{chunk_name} defines no function `{function}`"))?;
    let arg = lua.to_value(ctx).map_err(|e| e.to_string())?;
    let returned: LuaValue = func.call(arg).map_err(|e| e.to_string())?;

    let mut out = std::mem::take(&mut *effects.lock().unwrap());
    if !matches!(returned, LuaValue::Nil)
        && let Ok(v) = lua.from_value::<Value>(returned) {
            out.push(json!({"type": "data", "value": v}));
        }
    Ok(out)
}

/// Run one plugin action; the result is a JSON array of effects (or a single error effect).
pub fn run_action(plugin: &str, action: &str, ctx: &Value) -> Value {
    let safe = |s: &str| !s.is_empty() && !s.contains('/') && !s.contains("..");
    if !safe(plugin) {
        return json!([{"type": "toast", "message": "invalid plugin name", "kind": "error"}]);
    }
    let dir = plugins_dir().join(plugin);
    let Some(manifest) = read_manifest(&dir) else {
        return json!([{"type": "toast", "message": format!("plugin “{plugin}” not found"), "kind": "error"}]);
    };
    let Some(spec) = manifest.actions.iter().find(|a| a.id == action) else {
        return json!([{"type": "toast", "message": format!("{plugin} has no action “{action}”"), "kind": "error"}]);
    };
    let file = if spec.file.is_empty() { "main.lua" } else { spec.file.as_str() };
    let function = if spec.function.is_empty() { spec.id.as_str() } else { spec.function.as_str() };
    if !safe(file) {
        return json!([{"type": "toast", "message": "invalid script name", "kind": "error"}]);
    }
    let script = match std::fs::read_to_string(dir.join(file)) {
        Ok(s) => s,
        Err(e) => return json!([{"type": "toast", "message": format!("{file}: {e}"), "kind": "error"}]),
    };
    match run_lua(&script, &format!("{plugin}/{file}"), function, ctx, ACTION_TIME_LIMIT) {
        Ok(effects) => Value::Array(effects),
        Err(e) => {
            let msg = e.lines().next().unwrap_or("error").to_owned();
            json!([{"type": "toast", "message": format!("{plugin}: {msg}"), "kind": "error"}])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(script: &str, function: &str) -> Result<Vec<Value>, String> {
        run_lua(
            script,
            "test",
            function,
            &json!({"path": "/tmp", "cursor": "/tmp/a.txt", "selection": ["/tmp/a.txt"]}),
            Duration::from_secs(5),
        )
    }

    #[test]
    fn actions_emit_effects() {
        let effects = run(
            r#"function go(ctx) cfm.notify("hi " .. ctx.cursor); cfm.navigate("/usr") end"#,
            "go",
        )
        .unwrap();
        assert_eq!(effects[0]["type"], "toast");
        assert_eq!(effects[0]["message"], "hi /tmp/a.txt");
        assert_eq!(effects[1]["path"], "/usr");
    }

    #[test]
    fn returned_values_become_data() {
        let effects = run(r#"function go(ctx) return { count = #ctx.selection } end"#, "go").unwrap();
        assert_eq!(effects[0]["type"], "data");
        assert_eq!(effects[0]["value"]["count"], 1);
    }

    #[test]
    fn run_captures_output() {
        let effects = run(
            r#"function go(ctx) local r = cfm.run("echo", {"hello"}); return r.stdout end"#,
            "go",
        )
        .unwrap();
        assert_eq!(effects[0]["value"], "hello\n");
    }

    #[test]
    fn sandbox_has_no_os_or_io() {
        let err = run(r#"function go(ctx) os.execute("true") end"#, "go").unwrap_err();
        assert!(err.contains("os"), "{err}");
        assert!(run(r#"function go(ctx) return io == nil end"#, "go").is_ok());
    }

    #[test]
    fn runaway_scripts_are_stopped() {
        let started = Instant::now();
        let err = run_lua(
            "function go(ctx) while true do end end",
            "test",
            "go",
            &json!({}),
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(err.contains("too long"), "{err}");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn missing_function_is_reported() {
        let err = run("x = 1", "nope").unwrap_err();
        assert!(err.contains("nope"));
    }
}

#[cfg(test)]
mod install_tests {
    use super::*;
    use crate::testutil::Sandbox;

    fn install(sb: &Sandbox, name: &str, manifest: &str, lua: &str) {
        sb.touch(&format!(".config/captainfm/plugins/{name}/plugin.json"), manifest);
        sb.touch(&format!(".config/captainfm/plugins/{name}/main.lua"), lua);
    }

    #[test]
    fn lists_installed_plugins_with_panels_and_actions() {
        let sb = Sandbox::new("plugins-list");
        install(
            &sb,
            "demo",
            r#"{"name":"Demo","panels":[{"id":"p","title":"Panel","qml":"P.qml","side":"bottom"}],
                "actions":[{"id":"go","title":"Go","keys":["<Space> p g"]}]}"#,
            "function go() end",
        );
        install(&sb, "broken", "{ not json", "");
        let listed = list();
        let arr = listed.as_array().unwrap();
        assert_eq!(arr.len(), 1, "a plugin with an invalid manifest is skipped");
        assert_eq!(arr[0]["name"], "Demo");
        assert_eq!(arr[0]["panels"][0]["side"], "bottom");
        assert!(arr[0]["panels"][0]["qml"].as_str().unwrap().starts_with("file://"));
        assert_eq!(arr[0]["actions"][0]["keys"][0], "<Space> p g");
    }

    #[test]
    fn runs_an_installed_action() {
        let sb = Sandbox::new("plugins-run");
        install(
            &sb,
            "hello",
            r#"{"actions":[{"id":"hi","function":"greet"}]}"#,
            r#"function greet(ctx) cfm.notify("hi from " .. ctx.path); return 42 end"#,
        );
        let effects = run_action("hello", "hi", &json!({"path": "/tmp"}));
        assert_eq!(effects[0]["type"], "toast");
        assert_eq!(effects[0]["message"], "hi from /tmp");
        assert_eq!(effects[1]["value"], 42);
    }

    #[test]
    fn bad_requests_become_error_toasts() {
        let sb = Sandbox::new("plugins-errors");
        install(&sb, "p", r#"{"actions":[{"id":"a"}]}"#, "function a() error('boom') end");
        let kind = |v: Value| v[0]["kind"].as_str().unwrap().to_owned();
        assert_eq!(kind(run_action("../evil", "a", &json!({}))), "error");
        assert_eq!(kind(run_action("missing", "a", &json!({}))), "error");
        assert_eq!(kind(run_action("p", "nope", &json!({}))), "error");
        let failed = run_action("p", "a", &json!({}));
        assert_eq!(kind(failed.clone()), "error");
        assert!(failed[0]["message"].as_str().unwrap().contains("boom"));
    }
}
