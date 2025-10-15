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
    n.push("  command = $cc -MMD -MF $out.d $cflags $includes -c $in -o $out");
    n.push("  depfile = $out.d");
    n.push("  deps = gcc");
    n.push("");

    n.push("rule cxx");
    n.push("  command = $cxx -MMD -MF $out.d $cxxflags $includes -c $in -o $out");
    n.push("  depfile = $out.d");
    n.push("  deps = gcc");
    n.push("");

    n.push("rule ar");
    n.push("  command = $ar $arflags $out $in");
    n.push("");

    n.push("rule libtool_static");
    n.push("  command = libtool -static -o $out $in");
    n.push("");

    n.push("rule link_exe");
    n.push("  command = $link $linkflags $in -o $out $ldflags $libdirs $libs");
    n.push("");

    n.push("rule link_exe_msvc");
    n.push("  command = $link /OUT:$out $in $ldflags $libdirs $libs");
    n.push("");
}
