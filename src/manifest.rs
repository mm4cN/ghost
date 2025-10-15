use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{collections::HashMap, fs};

#[derive(Debug, serde::Deserialize)]
pub struct BuildDir {
    pub dir: String,
}

#[derive(Debug, Deserialize)]
pub struct ProjectRoot {
    pub project: Option<ProjectMeta>,
    pub workspace: Option<Workspace>,
    pub profile: Option<HashMap<String, ProfileFrag>>,
    pub build_dir: Option<BuildDir>,
}

#[derive(Debug, Deserialize)]
pub struct ProjectMeta {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Deserialize)]
pub struct Workspace {
    pub members: Vec<String>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct ProfileFrag {
    pub defines: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct PackageManifest {
    pub package: Package,
    pub sources: Sources,
    pub public: Option<PubPriv>,
    pub private: Option<PubPriv>,
    pub deps: Option<Deps>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Package {
    pub name: String,
    pub version: Option<String>,
    pub r#type: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Sources {
    pub files: Vec<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct PubPriv {
    pub include_dirs: Option<Vec<String>>,
    pub defines: Option<Vec<String>>,
    pub link_libs: Option<Vec<String>>,
    pub link_dirs: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Deps {
    pub direct: Option<Vec<String>>,
    pub private: Option<Vec<String>>,
}

pub fn load_root_manifest(path: &str) -> Result<ProjectRoot> {
    let txt = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let root: ProjectRoot = toml::from_str(&txt).with_context(|| "parse ghost.build")?;
    Ok(root)
}

pub fn load_package_manifest(path: &str) -> Result<PackageManifest> {
    let txt = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let pkg: PackageManifest = toml::from_str(&txt).with_context(|| "parse package ghost.build")?;
    Ok(pkg)
}

pub fn assert_package(pkg: &PackageManifest) -> Result<()> {
    let t = pkg.package.r#type.as_str();
    match t {
        "static" | "shared" | "interface" | "exe" | "test" => {}
        _ => bail!("unsupported package.type: {t}"),
    }
    if pkg.sources.files.is_empty() {
        bail!("sources.files must not be empty (explicit sources only)");
    }
    Ok(())
}
