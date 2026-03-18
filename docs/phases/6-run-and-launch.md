# Phase 6 — Run and Launch

Implement `rosup run` and `rosup launch` for executing ROS nodes and launch files.

## Background

`rosup run` and `rosup launch` delegate to `ros2 run` and `ros2 launch`
respectively, but must set up the correct environment first. The subprocess
needs both the user's workspace install and the dep layer visible.

Environment for `ros2` subprocesses:

```
AMENT_PREFIX_PATH = <project>/install
                  : <project>/.rosup/install   ← dep layer
                  : /opt/ros/{distro}         ← base (already in env)
                  : (existing)
```

Rather than sourcing `setup.bash` (which chains and can double-apply prefixes),
rosup sources `local_setup.bash` from each layer in order, which applies only
that layer's own packages without re-chaining.

## Work Items

### 6.1 Workspace Environment Setup

- [ ] Check `AMENT_PREFIX_PATH` is set; warn if base ROS not sourced
- [ ] Construct subprocess `AMENT_PREFIX_PATH` by prepending:
  1. `<project>/install` (user workspace — must be built first)
  2. `<project>/.rosup/install` (dep layer — only if it exists)
- [ ] If `<project>/install` does not exist, warn and suggest `rosup build`
- [ ] Propagate `CMAKE_PREFIX_PATH`, `PYTHONPATH`, `LD_LIBRARY_PATH` consistent
  with the ament prefix stack

### 6.2 `rosup run`

- [ ] Accept `<package_name> <executable_name>` positional args
- [ ] Run environment setup (6.1)
- [ ] Delegate to `ros2 run <package> <executable> [-- args]`
- [ ] Arguments after `--` are forwarded verbatim to the node
- [ ] Forward signals (SIGINT, SIGTERM) to the child process
- [ ] Propagate exit code from `ros2 run`

### 6.3 `rosup launch`

- [ ] Accept `<package_name> <launch_file>` positional args
- [ ] Run environment setup (6.1)
- [ ] Delegate to `ros2 launch <package> <launch_file> [-- args]`
- [ ] Arguments after `--` are forwarded verbatim
- [ ] Forward signals (SIGINT, SIGTERM) to the child process
- [ ] Propagate exit code from `ros2 launch`

## Acceptance Criteria

- [ ] `rosup run my_pkg my_node` launches the node with workspace and dep layer visible
- [ ] `rosup run my_pkg my_node -- --ros-args -p param:=value` passes args correctly
- [ ] `rosup launch my_pkg launch.py` launches with workspace and dep layer visible
- [ ] `rosup launch my_pkg launch.py -- arg:=value` passes args correctly
- [ ] Ctrl+C (SIGINT) cleanly terminates the child process
- [ ] Running before building prints a warning suggesting `rosup build`
- [ ] Packages from `<project>/.rosup/install/` are visible to the launched node
- [ ] `AMENT_PREFIX_PATH` is not double-applied for any prefix
- [ ] Exit code from `ros2 run` / `ros2 launch` propagates as rosup exit code
