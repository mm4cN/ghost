function before_generate(ctx)
  print("Calling before_generate hook")
end

function before_discover(ctx)
  print("Calling before_discover hook")
end

function before_build(ctx)
  print("Calling before_build hook")
end

function after_build(ctx)
  print("Calling after_build hook")
end

