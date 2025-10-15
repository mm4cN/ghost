use crate::context::Ctx;
use anyhow::Result;
use globset::{Glob, GlobSetBuilder};
use serde::Serialize;
use std::{fs, path::Path};
use walkdir::WalkDir;

#[derive(Debug, Serialize)]
pub struct FileList {
    pub files: Vec<String>,
}

pub fn discover(
    _ctx: &Ctx,
    pkg_root: &str,
    roots: &[String],
    include: &[String],
    exclude: &[String],
) -> Result<FileList> {
    let mut gb = GlobSetBuilder::new();
    for p in include {
        gb.add(Glob::new(p)?);
    }
    let inc = gb.build()?;

    let mut gb = GlobSetBuilder::new();
    for p in exclude {
        gb.add(Glob::new(p)?);
    }
    for d in ["**/.git/**", "**/build/**", "**/.ghost/**"] {
        gb.add(Glob::new(d)?);
    }
    let exc = gb.build()?;

    let mut out = vec![];
    for r in roots {
        let base = Path::new(pkg_root).join(r);
        if !base.exists() {
            continue;
        }
        for e in WalkDir::new(&base).into_iter().filter_map(|e| e.ok()) {
            if !e.file_type().is_file() {
                continue;
            }
            let rel =
                pathdiff::diff_paths(e.path(), pkg_root).unwrap_or_else(|| e.path().to_path_buf());
            let rels = rel.to_string_lossy().replace('\\', "/");
            if exc.is_match(&rels) {
                continue;
            }
            if inc.is_match(&rels) {
                out.push(rels);
            }
        }
    }

    out.sort();
    let list = FileList { files: out };
    let cache_dir = format!("{}/.ghost", pkg_root);
    fs::create_dir_all(&cache_dir)?;
    fs::write(
        format!("{}/files.json", cache_dir),
        serde_json::to_vec_pretty(&list)?,
    )?;
    Ok(list)
}
