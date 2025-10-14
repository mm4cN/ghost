use crate::context::Ctx;
use anyhow::Result;
use mlua::{Function, Lua, LuaSerdeExt, Value};
use std::fs;

fn exec_shell(cmdline: &str, cwd: &str) -> (i32, String, String) {
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("cmd");
    #[cfg(target_os = "windows")]
    let cmd = cmd.arg("/C").arg(cmdline).current_dir(cwd);

    #[cfg(not(target_os = "windows"))]
    let mut cmd = std::process::Command::new("sh");
    #[cfg(not(target_os = "windows"))]
    let cmd = cmd.arg("-lc").arg(cmdline).current_dir(cwd);

    match cmd.output() {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (-1, String::new(), format!("spawn error: {e}")),
    }
}

pub fn run_lua_hooks(mut ctx: Ctx, project_root: &str) -> Result<Ctx> {
    let path = format!("{project_root}/build.lua");
    if !std::path::Path::new(&path).exists() {
        return Ok(ctx);
    }

    let lua_src = fs::read_to_string(&path)?;
    let lua = Lua::new();
    let globals = lua.globals();

    let ctx_val = lua.to_value(&ctx)?;
    globals.set("ctx", ctx_val)?;

    let pr = project_root.to_string();
    let exec_fn = lua.create_function(move |lua_ctx, cmd: String| {
        let (code, out, err) = exec_shell(&cmd, &pr);
        let t = lua_ctx.create_table()?;
        t.set("code", code)?;
        t.set("stdout", out)?;
        t.set("stderr", err)?;
        Ok(t)
    })?;
    globals.set("exec", exec_fn)?;

    lua.load(&lua_src).set_name("build.lua")?.exec()?;

    let call = |name: &str| -> mlua::Result<()> {
        match globals.get::<_, Value>(name)? {
            Value::Function(f) => {
                let c: Value = globals.get("ctx")?;
                let f: Function = f;
                f.call::<_, ()>(c)?;
            }
            _ => {}
        };
        Ok(())
    };

    for h in [
        "before_discover",
        "before_generate",
        "before_build",
        "after_build",
    ] {
        let _ = call(h);
    }

    let new_ctx: Ctx = lua.from_value(globals.get("ctx")?)?;
    Ok(new_ctx)
}
