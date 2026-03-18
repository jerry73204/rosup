# Phase 6 — Run and Launch

Implement `rox run` and `rox launch` for executing ROS nodes and launch files.

## Background

`rox run` and `rox launch` delegate to `ros2 run` and `ros2 launch`
respectively, but must set up the correct environment first. The subprocess
needs both the user's workspace install and the dep layer visible.

Environment for `ros2` subprocesses:

```
AMENT_PREFIX_PATH = <project>/install
                  : <project>/.rox/install   ← dep layer
                  : /opt/ros/{distro}         ← base (already in env)
                  : (existing)
```

Rather than sourcing `setup.bash` (which chains and can double-apply prefixes),
rox sources `local_setup.bash` from each layer in order, which applies only
that layer's own packages without re-chaining.

## Work Items

### 6.1 Workspace Environment Setup

- [ ] Check `AMENT_PREFIX_PATH` is set; warn if base ROS not sourced
- [ ] Construct subprocess `AMENT_PREFIX_PATH` by prepending:
  1. `<project>/install` (user workspace — must be built first)
  2. `<project>/.rox/install` (dep layer — only if it exists)
- [ ] If `<project>/install` does not exist, warn and suggest `rox build`
- [ ] Propagate `CMAKE_PREFIX_PATH`, `PYTHONPATH`, `LD_LIBRARY_PATH` consistent
  with the ament prefix stack

### 6.2 `rox run`

- [ ] Accept `<package_name> <executable_name>` positional args
- [ ] Run environment setup (6.1)
- [ ] Delegate to `ros2 run <package> <executable> [-- args]`
- [ ] Arguments after `--` are forwarded verbatim to the node
- [ ] Forward signals (SIGINT, SIGTERM) to the child process
- [ ] Propagate exit code from `ros2 run`

### 6.3 `rox launch`

- [ ] Accept `<package_name> <launch_file>` positional args
- [ ] Run environment setup (6.1)
- [ ] Delegate to `ros2 launch <package> <launch_file> [-- args]`
- [ ] Arguments after `--` are forwarded verbatim
- [ ] Forward signals (SIGINT, SIGTERM) to the child process
- [ ] Propagate exit code from `ros2 launch`

## Acceptance Criteria

- [ ] `rox run my_pkg my_node` launches the node with workspace and dep layer visible
- [ ] `rox run my_pkg my_node -- --ros-args -p param:=value` passes args correctly
- [ ] `rox launch my_pkg launch.py` launches with workspace and dep layer visible
- [ ] `rox launch my_pkg launch.py -- arg:=value` passes args correctly
- [ ] Ctrl+C (SIGINT) cleanly terminates the child process
- [ ] Running before building prints a warning suggesting `rox build`
- [ ] Packages from `<project>/.rox/install/` are visible to the launched node
- [ ] `AMENT_PREFIX_PATH` is not double-applied for any prefix
- [ ] Exit code from `ros2 run` / `ros2 launch` propagates as rox exit code
