function before_generate(ctx)
  local r = exec("git rev-parse --short HEAD || echo dev")
  local rev = (r.stdout or "dev"):gsub("%s+$", "")
  os.execute("mkdir -p include && echo '#define GHOST_REV \"" .. rev .. "\"' > include/version.h")
  table.insert(ctx.profile.defines, "GHOST_REV_STR=\"" .. rev .. "\"")
  table.insert(ctx.discover_include, "include/version.h")
  ctx.log[#ctx.log + 1] = "Injected GHOST_REV = " .. rev
end
