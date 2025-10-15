# Ghost (in the Shell) 👻

<div align="center">
<img src="assets/ghost.png" alt="Ghost logo" width="180">
</div>

*A minimal, deterministic build system for C/C++ (and friends). No magic — just explicit inputs, fast rebuilds, and clean graphs.*

> “If a build isn’t deterministic, it’s a horoscope.”

---

## TL;DR

- **Explicit sources only** – no globs; you list every source file.
- **Configurable `builddir`** – artifacts and `build.ninja` are generated under a user-defined directory (default: `build/`).
- **Per-rule `-I` propagation** – include paths from the package and its public deps are injected into every compile rule.
- **Linker from profile** – choose `clang++/g++`, `ld.lld/mold`, or MSVC `link` via `ghost.profile`.
- **Sandboxed Lua hooks (optional)** – flexibility without foot-guns.
- **`compile_commands.json` in the repo root** – your IDE/clangd sees exactly what Ninja executes.

---

## Project Status (MVP)

✅ Generate `<builddir>/build.ninja`  
✅ 1 translation unit ⇒ 1 object file  
✅ Static libraries via `ar`/`libtool` + executable linking  
✅ `-I` from package and public dependencies  
✅ `compile_commands.json` written to **repo root**  
✅ Toolchain & linker configured via `ghost.profile`  
✅ `[builddir]` in root `ghost.build`  
⚠️ WIP: shared libs, test targets, native scheduler (instead of Ninja), remote cache

---

## Requirements

- **Rust** 1.75+ (to build the CLI)
- **Ninja** (executor)
- C/C++ toolchain (Clang/GCC/MSVC), optional `libtool` on macOS

---

## Quick Start

```bash
# 1) Build the CLI
cargo build --release
alias ghost=./target/release/ghost-in-the-shell

# 2) (Optional) set your toolchain/linker profile
#    If you don't, Ghost defaults to clang/clang++ and sensible flags.
cp test_project/ghost.profile ghost.profile  # or create your own

# 3) Build (generates <builddir>/build.ninja and runs ninja -f ...)
ghost build

# 4) Validate explicit sources & paths
ghost discover
```

## Repository Layout (example)

```bash
test_project/
├── apps
│   ├── app.cpp
│   └── ghost.build
├── build.lua
├── ghost.build
├── ghost.profile
└── libs
    ├── add
    │   ├── add.c
    │   ├── ghost.build
    │   └── include
    │       └── add.h
    └── io
        ├── ghost.build
        ├── include
        │   └── io.hpp
        └── io.cpp

```

## Manifest Files

### Root `ghost.build`
```bash
[project]
name = "Ghost (in the Shell)"
version = "0.1.0"

[workspace]
members = ["libs/math", "libs/io", "apps"]

# Where to place build artifacts and the generated build.ninja
[builddir]
dir = "build"            # if omitted, defaults to "build"

# Optional project profiles (flags, excludes)
[profile.debug]
defines = ["DEBUG=1"]
```

### Package `ghost.build`

```bash
# add
[package]
name = "add"
type = "static"          # static | exe  (todo: shared | test)

[sources]
files = [
  "src/add.cpp",
]

[public]
include_dirs = ["include"]  # exported as -I to dependents

# io
[package]
name = "io"
type = "static"

[sources]
files = [
  "io.cpp"
]

[public]
include_dirs = ["include"]

```

### Executable `ghost.build`

```bash
[package]
name = "adder"
type = "exe"

[deps]
direct = ["add", "io"]

[sources]
files = [
  "app.cpp"
]
```

### Toolchain `ghost.profile`

```bash
[toolchain]
cc  = "clang"
cxx = "clang++"
ar  = "ar"
arflags = ["rcs"]

# Linker selection
link_mode = "driver"        # "driver" | "ld" | "msvc"
link_cxx  = "clang++"       # used when link_mode = "driver"
# link_mode = "ld"
# link = "ld.lld"
# fuse_ld = "mold"

ldflags = ["-Wl,-rpath,$ORIGIN/../lib"]
libdirs = ["build/lib"]
libs    = []
```

## License

MIT
