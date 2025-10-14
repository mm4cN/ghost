use crate::context::{Profile, Toolchain};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct ProfileFile {
    pub toolchain: Toolchain,
    pub env: Option<std::collections::HashMap<String, String>>, // future use
}

pub fn load_profile(path: &str) -> Result<ProfileFile> {
    let txt = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let pf: ProfileFile = toml::from_str(&txt).with_context(|| "parse ghost.profile")?;
    Ok(pf)
}

pub fn default_profile() -> (Toolchain, Profile) {
    (
        Toolchain {
            cc: "clang".into(),
            cxx: "clang++".into(),
            ar: "llvm-ar".into(),
            rc: None,
            sysroot: None,
            target_triple: None,
            cflags: vec!["-Wall".into(), "-Wextra".into()],
            cxxflags: vec!["-std=c++20".into(), "-O2".into()],
            ldflags: vec![],
        },
        Profile {
            name: "debug".into(),
            defines: vec!["DEBUG=1".into()],
            exclude: vec![],
        },
    )
}
