use anyhow::Result;
use std::fs;

pub struct NinjaBuf(String);
impl NinjaBuf {
    pub fn new() -> Self {
        Self(String::new())
    }
    pub fn push(&mut self, s: &str) {
        self.0.push_str(s);
        self.0.push('\n');
    }
    pub fn write_to(&self, path: &str) -> Result<()> {
        fs::write(path, &self.0)?;
        Ok(())
    }
}

pub fn emit_prelude(n: &mut NinjaBuf) {
    n.push("rule cc");
    n.push(" command = $cc -MMD -MF $out.d $cflags $includes -c $in -o $out");
    n.push(" depfile = $out.d");
    n.push(" deps = gcc");
    n.push("");

    n.push("rule cxx");
    n.push(" command = $cxx -MMD -MF $out.d $cxxflags $includes -c $in -o $out");
    n.push(" depfile = $out.d");
    n.push(" deps = gcc");
    n.push("");

    n.push("rule ar");
    n.push(" command = $ar rcs $out $in");
    n.push("");

    n.push("rule link_exe");
    n.push(" command = $cxx $in -o $out $ldflags $libdirs $libs");
    n.push("");
}
