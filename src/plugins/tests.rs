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
    let effects = run(
        r#"function go(ctx) return { count = #ctx.selection } end"#,
        "go",
    )
    .unwrap();
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
