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
use serde::Serialize;
use std::{collections::HashMap, env, fs, path::Path, path::PathBuf};

#[derive(Serialize)]
struct CompileCommand {
    directory: String,
    file: String,
    command: String,
    output: String,
}

#[derive(Clone, Default)]
struct DepMeta {
    root: std::path::PathBuf,
    public_includes: Vec<String>,
}

fn write_compdb(root_dir: &str, entries: &[CompileCommand]) -> anyhow::Result<()> {
    let path = std::path::Path::new(root_dir).join("compile_commands.json");
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(&path, json)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

fn is_compile_src(p: &str) -> bool {
    matches!(
        std::path::Path::new(p).extension().and_then(|s| s.to_str()),
        Some("c" | "cc" | "cpp" | "cxx")
    )
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
    println!(
        "Build dir: {}",
        root.build_dir
            .as_ref()
            .map(|b| b.dir.clone())
            .unwrap_or_else(|| "build".into())
    );
    println!("Targets: ");

    let members = root
        .workspace
        .ok_or_else(|| anyhow::anyhow!("workspace.members missing"))?
        .members;

    for m in members {
        let pkg_root = PathBuf::from(&m).canonicalize()?;
        let pkg = load_package_manifest(pkg_root.join("ghost.build").to_str().unwrap())?;
        manifest::assert_package(&pkg)?;

        let mut missing = Vec::new();
        let mut total = 0usize;
        let mut compilable = 0usize;

        for f in &pkg.sources.files {
            total += 1;
            let exists = pkg_root.join(f).exists();
            if !exists {
                missing.push(f.clone());
            }
            if is_compile_src(f) {
                compilable += 1;
            }
        }

        if missing.is_empty() {
            println!(
                "{}: {} files ({} compilable) – OK",
                pkg.package.name, total, compilable
            );
        } else {
            eprintln!("{}: missing {} file(s):", pkg.package.name, missing.len());
            for x in missing {
                eprintln!("  - {}", x);
            }
            std::process::exit(2);
        }
    }
    Ok(())
}

fn cmd_build(profile_arg: Option<&str>) -> Result<()> {
    let (tc, prof) = load_profile_chain(profile_arg)?;
    let mut ctx = base_ctx()?;
    let ws_root = ctx.workspace_root.clone();
    let mut ccdb: Vec<CompileCommand> = Vec::new();
    ctx.toolchain = tc;
    ctx.profile = prof;
    ctx = hooks::run_lua_hooks(ctx, &ws_root)?;

    let root = load_root_manifest("ghost.build").context("load root ghost.build")?;
    let build_dir = root
        .build_dir
        .as_ref()
        .map(|b| b.dir.clone())
        .unwrap_or_else(|| "build".into());
    let members = root
        .workspace
        .ok_or_else(|| anyhow::anyhow!("workspace.members missing"))?
        .members;

    let dep_map = collect_dep_meta(&members)?;

    let mut nin = ninja::NinjaBuf::new();
    ninja::emit_prelude(&mut nin);
    nin.push(&format!("builddir = {}", build_dir));
    nin.push(&format!("cc = {}", ctx.toolchain.cc));
    nin.push(&format!("cxx = {}", ctx.toolchain.cxx));
    nin.push(&format!("ar = {}", ctx.toolchain.ar));
    let arflags = ctx.toolchain.arflags.clone().unwrap_or_default().join(" ");
    nin.push(&format!("arflags = {}", arflags));
    nin.push(&format!("cflags = {}", ctx.toolchain.cflags.join(" ")));
    nin.push(&format!("cxxflags = {}", ctx.toolchain.cxxflags.join(" ")));
    nin.push(&format!("ldflags = {}", ctx.toolchain.ldflags.join(" ")));

    let libdirs = ctx
        .toolchain
        .libdirs
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|d| format!("-L{}", d))
        .collect::<Vec<_>>()
        .join(" ");
    nin.push(&format!("libdirs = {}", libdirs));

    let libs = ctx
        .toolchain
        .libs
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|l| {
            if l.starts_with("-l") {
                l
            } else {
                format!("-l{}", l)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    nin.push(&format!("libs = {}", libs));

    let link_mode = ctx.toolchain.link_mode.as_deref().unwrap_or("driver");
    match link_mode {
        "driver" => {
            let linker = ctx
                .toolchain
                .link_cxx
                .clone()
                .unwrap_or_else(|| ctx.toolchain.cxx.clone());
            nin.push(&format!("link = {}", linker));
            if let Some(f) = ctx.toolchain.fuse_ld.as_deref() {
                nin.push(&format!("linkflags = -fuse-ld={}", f));
            } else {
                nin.push("linkflags =".into());
            }
        }
        "ld" => {
            let linker = ctx.toolchain.link.clone().unwrap_or_else(|| "ld".into());
            nin.push(&format!("link = {}", linker));
            nin.push("linkflags =".into());
        }
        "msvc" => {
            let linker = ctx
                .toolchain
                .link_cxx
                .clone()
                .unwrap_or_else(|| "link".into());
            nin.push(&format!("link = {}", linker));
            nin.push("linkflags =".into());
        }
        _ => {
            // fallback: driver
            nin.push(&format!("link = {}", ctx.toolchain.cxx));
            nin.push("linkflags =".into());
        }
    }
    nin.push("");

    fs::create_dir_all(format!("{}/obj", build_dir)).ok();
    fs::create_dir_all(format!("{}/lib", build_dir)).ok();
    fs::create_dir_all(format!("{}/bin", build_dir)).ok();

    let mut libdirs_vec = ctx.toolchain.libdirs.clone().unwrap_or_default();
    let default_libdir = format!("{}/lib", build_dir);
    if !libdirs_vec.iter().any(|d| d == &default_libdir) {
        libdirs_vec.push(default_libdir);
    }
    let libdirs = libdirs_vec
        .into_iter()
        .map(|d| format!("-L{}", d))
        .collect::<Vec<_>>()
        .join(" ");
    nin.push(&format!("libdirs = {}", libdirs));

    let libs = ctx
        .toolchain
        .libs
        .clone()
        .unwrap_or_default()
        .into_iter()
        .map(|l| {
            if l.starts_with("-l") {
                l
            } else {
                format!("-l{}", l)
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    nin.push(&format!("libs = {}", libs));
    nin.push("");

    let mut built_libs: Vec<String> = vec![];
    for m in members {
        let pkg_root = PathBuf::from(&m).canonicalize()?;
        let pkg = load_package_manifest(pkg_root.join("ghost.build").to_str().unwrap())?;
        assert_package(&pkg)?;

        let pkg_obj_dir = format!("build/obj/{}", pkg.package.name);
        std::fs::create_dir_all(&pkg_obj_dir).ok();

        use std::collections::BTreeMap;
        let mut unit_map: BTreeMap<String, String> = BTreeMap::new();

        for f in &pkg.sources.files {
            if !is_compile_src(f) {
                continue;
            }

            let rule = if f.ends_with(".c") { "cc" } else { "cxx" };
            let pkg_obj_dir = format!("{}/obj/{}", build_dir, pkg.package.name);
            std::fs::create_dir_all(&pkg_obj_dir).ok();

            let obj = format!(
                "{}/{}.o",
                pkg_obj_dir,
                f.replace(['/', '\\'], "_").replace('.', "_")
            );
            let inc = include_dirs_vars(&pkg, &pkg_root, &dep_map);
            let src_abs = pkg_root
                .join(&f)
                .canonicalize()
                .unwrap_or(pkg_root.join(&f));
            let obj_abs = std::path::Path::new(&obj)
                .canonicalize()
                .unwrap_or(std::path::PathBuf::from(&obj));
            let (compiler, flags) = if rule == "cc" {
                (ctx.toolchain.cc.clone(), ctx.toolchain.cflags.clone())
            } else {
                (ctx.toolchain.cxx.clone(), ctx.toolchain.cxxflags.clone())
            };
            // TODO add MSVC
            // cl /nologo /showIncludes <FLAGS> <INCLUDES> /c <FILE> /Fo<OBJ>
            let command = format!(
                "{} -MMD -MF {}.d {} {} -c {} -o {}",
                compiler,
                obj,
                flags.join(" "),
                inc,
                src_abs.display(),
                obj_abs.display(),
            );

            nin.push(&format!("build {obj}: {rule} {}/{}", pkg_root.display(), f));
            nin.push(&format!("  includes = {}", inc));
            unit_map.insert(f.clone(), obj);

            ccdb.push(CompileCommand {
                directory: ws_root.clone(),
                file: src_abs.display().to_string(),
                command,
                output: obj_abs.display().to_string(),
            });
        }

        let objs: Vec<String> = unit_map.values().cloned().collect();
        if objs.is_empty() {
            eprintln!(
                "error: package '{}' has no compilable sources in [sources.files]",
                pkg.package.name
            );
            std::process::exit(2);
        }

        match pkg.package.r#type.as_str() {
            "static" => {
                let out = format!("{}/lib/lib{}.a", build_dir, pkg.package.name);
                if ctx.toolchain.ar.ends_with("libtool") {
                    nin.push(&format!("build {}: libtool_static {}", out, objs.join(" ")));
                } else {
                    nin.push(&format!("build {}: ar {}", out, objs.join(" ")));
                }
                built_libs.push(out);
            }
            "exe" => {
                let mut inputs = objs.clone();
                inputs.extend(built_libs.clone());
                let out = format!("{}/bin/{}", build_dir, pkg.package.name);
                let link_rule = if ctx.toolchain.link_mode.as_deref() == Some("msvc") {
                    "link_exe_msvc"
                } else {
                    "link_exe"
                };
                nin.push(&format!(
                    "build {}: {} {}",
                    out,
                    link_rule,
                    inputs.join(" ")
                ));
                nin.push(&format!("  libdirs = -L{}/lib", build_dir));
            }
            "interface" | "shared" | "test" => {
                // TODO
            }
            other => eprintln!("warn: unsupported package type '{}'", other),
        }
    }

    let build_ninja_path = format!("{}/build.ninja", build_dir);
    nin.write_to(&build_ninja_path)?;

    write_compdb(&ws_root, &ccdb)?;

    // ninja z użyciem -f, żeby nie musieć chdir
    let status = std::process::Command::new("ninja")
        .args(["-f", &build_ninja_path])
        .status()
        .context("run ninja")?;
    if !status.success() {
        bail!("ninja failed");
    }
    Ok(())
}

fn include_dirs_vars(
    pkg: &manifest::PackageManifest,
    pkg_root: &Path,
    dep_map: &HashMap<String, DepMeta>,
) -> String {
    let mut incs: Vec<String> = Vec::new();

    for d in ["include", "src"] {
        let p = pkg_root.join(d);
        if p.exists() {
            incs.push(format!("-I\"{}\"", p.display()));
        }
    }
    let gen = pkg_root.join(".gen");
    if gen.exists() {
        incs.push(format!("-I\"{}\"", gen.display()));
    }

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

    if let Some(deps) = &pkg.deps {
        if let Some(list) = &deps.direct {
            for dep_name in list {
                if let Some(meta) = dep_map.get(dep_name) {
                    for d in &meta.public_includes {
                        let p = meta.root.join(d);
                        incs.push(format!("-I\"{}\"", p.display()));
                    }
                    let def_inc = meta.root.join("include");
                    if def_inc.exists() {
                        incs.push(format!("-I\"{}\"", def_inc.display()));
                    }
                    let dep_gen = meta.root.join(".gen");
                    if dep_gen.exists() {
                        incs.push(format!("-I\"{}\"", dep_gen.display()));
                    }
                }
            }
        }
    }

    incs.sort();
    incs.dedup();
    incs.join(" ")
}
