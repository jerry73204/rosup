# Phase 6 — Run and Launch

Implement `rox run` and `rox launch` for executing ROS nodes and launch files.

## Work Items

- [ ] Implement workspace environment sourcing
  - [ ] Locate `install/setup.bash` (or `.sh`, `.zsh`) relative to project root
  - [ ] Source the setup script before executing ros2 commands
  - [ ] Also source `~/.rox/install/setup.bash` for source-pulled deps
- [ ] Implement `rox run`
  - [ ] Resolve dependencies
  - [ ] Build if needed (or warn if not built)
  - [ ] Delegate to `ros2 run <package> <executable>`
  - [ ] Pass through arguments after `--`
  - [ ] Forward signals (SIGINT, SIGTERM) to the child process
- [ ] Implement `rox launch`
  - [ ] Resolve dependencies
  - [ ] Build if needed (or warn if not built)
  - [ ] Delegate to `ros2 launch <package> <launch_file>`
  - [ ] Pass through arguments after `--`
  - [ ] Forward signals to the child process

## Acceptance Criteria

- [ ] `rox run my_pkg my_node` launches the node with workspace install sourced
- [ ] `rox run my_pkg my_node -- --ros-args -p param:=value` passes args correctly
- [ ] `rox launch my_pkg launch.py` launches with workspace install sourced
- [ ] `rox launch my_pkg launch.py -- arg:=value` passes args correctly
- [ ] Ctrl+C (SIGINT) cleanly terminates the child process
- [ ] Running before building prints a warning suggesting `rox build`
- [ ] Environment from both workspace `install/` and `~/.rox/install/` are available
- [ ] Exit code from ros2 run/launch propagates as rox exit code
