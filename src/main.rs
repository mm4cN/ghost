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
use std::{collections::HashMap, env, fs, path::PathBuf, process::Command};

#[derive(Clone, Default)]
struct DepMeta {
    root: std::path::PathBuf,
    public_includes: Vec<String>,
}

fn collect_dep_meta(members: &[String]) -> anyhow::Result<HashMap<String, DepMeta>> {
    let mut map = HashMap::new();
    for m in members {
        let pkg_root = std::path::PathBuf::from(m).canonicalize()?;
        let pkg = manifest::load_package_manifest(pkg_root.join("ghost.build").to_str().unwrap())?;
        let pub_inc = pkg
            .public
            .as_ref()
            .and_then(|p| p.include_dirs.clone())
            .unwrap_or_default();
        map.insert(
            pkg.package.name.clone(),
            DepMeta {
                root: pkg_root,
                public_includes: pub_inc,
            },
        );
    }
    Ok(map)
}

fn include_flags(
    pkg: &manifest::PackageManifest,
    pkg_root: &std::path::Path,
    dep_map: &HashMap<String, DepMeta>,
) -> String {
    let mut incs: Vec<String> = Vec::new();

    // Domyślne katalogi pakietu
    for d in ["include", "src"].iter() {
        let p = pkg_root.join(d);
        if p.exists() {
            incs.push(format!("-I\"{}\"", p.display()));
        }
    }
    let gen = pkg_root.join(".gen");
    if gen.exists() {
        incs.push(format!("-I\"{}\"", gen.display()));
    }

    // Deklaratywne include_dirs (public + private) – prefiksuj rootem pakietu
    let push_dirs = |dirs: Option<Vec<String>>, incs: &mut Vec<String>| {
        if let Some(v) = dirs {
            for d in v {
                let p = pkg_root.join(&d);
                incs.push(format!("-I\"{}\"", p.display()));
            }
        }
    };
    if let Some(pv) = &pkg.public {
        push_dirs(pv.include_dirs.clone(), &mut incs);
    }
    if let Some(pv) = &pkg.private {
        push_dirs(pv.include_dirs.clone(), &mut incs);
    }

    // Publiczne include’y zależności (direct)
    if let Some(deps) = &pkg.deps {
        if let Some(list) = &deps.direct {
            for dep_name in list {
                if let Some(meta) = dep_map.get(dep_name) {
                    // z manifestu
                    for d in &meta.public_includes {
                        let p = meta.root.join(d);
                        incs.push(format!("-I\"{}\"", p.display()));
                    }
                    // domyślne include/ zależności
                    let def_inc = meta.root.join("include");
                    if def_inc.exists() {
                        incs.push(format!("-I\"{}\"", def_inc.display()));
                    }
                }
            }
        }
    }

    incs.sort();
    incs.dedup();
    incs.join(" ")
}

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
    let dep_map = collect_dep_meta(&members)?;
    let mut nin = ninja::NinjaBuf::new();
    ninja::emit_prelude(&mut nin);

    nin.push(&format!("cc = {}", ctx.toolchain.cc));
    nin.push(&format!("cxx = {}", ctx.toolchain.cxx));
    nin.push(&format!("ar = {}", ctx.toolchain.ar));
    let arflags = ctx.toolchain.arflags.clone().unwrap_or_default().join(" ");
    nin.push(&format!("arflags = {}", arflags));
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
                "build/.obj/{}/{}.o",
                pkg.package.name,
                f.replace('/', "_").replace('.', "_")
            );
            let rule = if f.ends_with(".c") { "cc" } else { "cxx" };
            let inc = include_flags(&pkg, &pkg_root, &dep_map);
            nin.push(&format!("build {obj}: {rule} {}/{}", pkg_root.display(), f));
            nin.push(&format!("  includes = {}", inc));
            objs.push(obj);
        }

        if objs.is_empty() {
            eprintln!(
                "error: package '{}' has no object files; check [sources.include]/[roots]",
                pkg.package.name
            );
            std::process::exit(2);
        }

        match pkg.package.r#type.as_str() {
            "static" => {
                let out = format!("lib/lib{}.a", pkg.package.name);
                fs::create_dir_all("build/lib").ok();
                if ctx.toolchain.ar.ends_with("libtool") {
                    nin.push(&format!("build {}: libtool_static {}", out, objs.join(" ")));
                } else {
                    nin.push(&format!("build {}: ar {}", out, objs.join(" ")));
                }
                nin.push(&format!("build {out}: ar {}", objs.join(" ")));
                all_libs.push(out);
            }
            "exe" => {
                let mut inputs = objs.clone();
                inputs.extend(all_libs.clone());
                let out = format!("build/bin/{}", pkg.package.name);
                fs::create_dir_all("build/bin").ok();
                if ctx.toolchain.ar.ends_with("libtool") {
                    nin.push(&format!("build {}: libtool_static {}", out, objs.join(" ")));
                } else {
                    nin.push(&format!("build {}: ar {}", out, objs.join(" ")));
                }
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
