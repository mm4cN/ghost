use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Toolchain {
    pub cc: String,
    pub cxx: String,
    pub ar: String,
    pub rc: Option<String>,
    pub sysroot: Option<String>,
    pub target_triple: Option<String>,
    pub cflags: Vec<String>,
    pub cxxflags: Vec<String>,
    pub ldflags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Profile {
    pub name: String,
    pub defines: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Ctx {
    pub os: String,
    pub env: String,
    pub project_root: String,
    pub workspace_root: String,
    pub toolchain: Toolchain,
    pub profile: Profile,
    pub discover_roots: Vec<String>,
    pub discover_include: Vec<String>,
    pub discover_exclude: Vec<String>,
    pub log: Vec<String>,
}

