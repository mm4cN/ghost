mod context;
mod discover;
mod hooks;
mod manifest;
mod ninja;
mod profile;

use anyhow::{bail, Context as _, Result};
use context::{Ctx, Profile as CtxProfile};
use manifest::{assert_package, load_package_manifest, load_root_manifest};
use profile::{default_profile, load_profile};
use std::{env, fs, path::PathBuf, process::Command};

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match cmd {
        "help" => help(),
        "build" => cmd_build(args.get(2).map(|s| s.as_str()))?,
        "discover" => cmd_discover()?,
        _ => help(),
    }
    Ok(())
}

fn help() {
    println!(
        "Ghost – minimal build orchestrator\nUsage: ghost [build|discover|help] [--profile <file>]"
    );
}

fn load_profile_chain(opt: Option<&str>) -> Result<(context::Toolchain, CtxProfile)> {
    if let Some(p) = opt {
        return Ok((
            load_profile(p)?.toolchain,
            CtxProfile {
                name: "custom".into(),
                defines: vec![],
                exclude: vec![],
            },
        ));
    }
    if let Ok(p) = env::var("GHOST_PROFILE") {
        return Ok((
            load_profile(&p)?.toolchain,
            CtxProfile {
                name: "env".into(),
                defines: vec![],
                exclude: vec![],
            },
        ));
    }
    Ok(default_profile())
}

fn base_ctx() -> Result<Ctx> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    Ok(Ctx {
        os: if cfg!(target_os = "windows") {
            "windows".into()
        } else if cfg!(target_os = "macos") {
            "macos".into()
        } else {
            "linux".into()
        },
        env: std::env::var("GHOST_ENV").unwrap_or_else(|_| "dev".into()),
        project_root: cwd.display().to_string(),
        workspace_root: cwd.display().to_string(),
        ..Default::default()
    })
}

fn cmd_discover() -> Result<()> {
    let root = load_root_manifest("ghost.build").context("load root ghost.build")?;
    let members = root
        .workspace
        .ok_or_else(|| anyhow::anyhow!("workspace.members missing"))?
        .members;
    for m in members {
        let p = PathBuf::from(&m).join("ghost.build");
        let pkg = load_package_manifest(p.to_str().unwrap())?;
        manifest::assert_package(&pkg)?;
        let pkg_root = PathBuf::from(&m).canonicalize()?;
        let ctx = base_ctx()?;
        let list = discover::discover(
            &ctx,
            pkg_root.to_str().unwrap(),
            &pkg.sources.roots,
            &pkg.sources.include,
            &pkg.sources.exclude.clone().unwrap_or_default(),
        )?;
        println!("discovered {} files in {}", list.files.len(), m);
    }
    Ok(())
}

fn cmd_build(profile_arg: Option<&str>) -> Result<()> {
    let (tc, prof) = load_profile_chain(profile_arg)?;
    let mut ctx = base_ctx()?;
    let ws_root = ctx.workspace_root.clone();
    ctx.toolchain = tc;
    ctx.profile = prof;
    ctx = hooks::run_lua_hooks(ctx, &ws_root)?;

    let root = load_root_manifest("ghost.build").context("load root ghost.build")?;
    let members = root
        .workspace
        .ok_or_else(|| anyhow::anyhow!("workspace.members missing"))?
        .members;

    let mut nin = ninja::NinjaBuf::new();
    ninja::emit_prelude(&mut nin);

    nin.push(&format!("cc = {}", ctx.toolchain.cc));
    nin.push(&format!("cxx = {}", ctx.toolchain.cxx));
    nin.push(&format!("ar = {}", ctx.toolchain.ar));
    nin.push(&format!("cflags = {}", ctx.toolchain.cflags.join(" ")));
    nin.push(&format!("cxxflags = {}", ctx.toolchain.cxxflags.join(" ")));
    nin.push(&format!("ldflags = {}", ctx.toolchain.ldflags.join(" ")));
    nin.push("");

    let mut all_libs: Vec<String> = vec![];

    for m in members {
        let pkg_root = PathBuf::from(&m).canonicalize()?;
        let pkg = load_package_manifest(pkg_root.join("ghost.build").to_str().unwrap())?;
        assert_package(&pkg)?;
        let fl = discover::discover(
            &ctx,
            pkg_root.to_str().unwrap(),
            &pkg.sources.roots,
            &pkg.sources.include,
            &pkg.sources.exclude.clone().unwrap_or_default(),
        )?;

        let mut objs = vec![];
        for f in fl.files {
            if !(f.ends_with(".c") || f.ends_with(".cpp")) {
                continue;
            }
            let obj = format!(
                ".obj/{}/{}.o",
                pkg.package.name,
                f.replace('/', "_").replace('.', "_")
            );
            let rule = if f.ends_with(".c") { "cc" } else { "cxx" };
            nin.push(&format!("build {obj}: {rule} {}/{}", pkg_root.display(), f));
            nin.push(&format!(" includes = {}", include_dirs_vars(&pkg)));
            objs.push(obj);
        }

        match pkg.package.r#type.as_str() {
            "static" => {
                let out = format!("lib/lib{}.a", pkg.package.name);
                fs::create_dir_all("lib").ok();
                nin.push(&format!("build {out}: ar {}", objs.join(" ")));
                all_libs.push(out);
            }
            "exe" => {
                let mut inputs = objs.clone();
                inputs.extend(all_libs.clone());
                let out = format!("bin/{}", pkg.package.name);
                fs::create_dir_all("bin").ok();
                nin.push(&format!("build {out}: link_exe {}", inputs.join(" ")));
                nin.push(" libdirs = -Llib");
            }
            "interface" | "shared" | "test" => {
                // TODO
            }
            _ => {}
        }
    }

    nin.write_to("build.ninja")?;

    let status = Command::new("ninja").status().context("run ninja")?;
    if !status.success() {
        bail!("ninja failed")
    }
    Ok(())
}

fn include_dirs_vars(pkg: &manifest::PackageManifest) -> String {
    let mut incs = vec![];
    if let Some(p) = &pkg.public {
        if let Some(d) = &p.include_dirs {
            for x in d {
                incs.push(format!("-I{}", x));
            }
        }
    }
    if let Some(p) = &pkg.private {
        if let Some(d) = &p.include_dirs {
            for x in d {
                incs.push(format!("-I{}", x));
            }
        }
    }
    incs.join(" ")
}
