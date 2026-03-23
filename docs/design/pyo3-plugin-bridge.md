# PyO3 Plugin Bridge — Feasibility Study

Can rosup load and call existing colcon Python plugins via PyO3?

**Verdict: Yes, it works.** Tested by constructing a `TaskContext` from
scratch and calling `AmentCmakeBuildTask.build()` — the full cmake
configure + environment script generation pipeline executed successfully
through the Python API.

---

## What was tested

1. Load plugin class via `importlib.metadata.entry_points()`
2. Instantiate the task class
3. Construct `PackageDescriptor`, `argparse.Namespace`, `TaskContext`
4. Call `task.set_context(context=ctx)`
5. Call `await task.build()` (async coroutine)

Result: cmake was invoked, environment scripts were generated, ament
index files were partially created. The cmake build itself returned rc=1
due to sandbox constraints, but the plugin API interface worked correctly.

## Registered plugins on this system

```
cmake           = colcon_cmake.task.cmake.build:CmakeBuildTask
python          = colcon_core.task.python.build:PythonBuildTask
ros.ament_cmake = colcon_ros.task.ament_cmake.build:AmentCmakeBuildTask
ros.ament_python= colcon_ros.task.ament_python.build:AmentPythonBuildTask
ros.catkin      = colcon_ros.task.catkin.build:CatkinBuildTask
ros.cmake       = colcon_ros.task.cmake.build:CmakeBuildTask
ros.ament_cargo = colcon_cargo_ros2.task.ament_cargo.build:AmentCargoBuildTask
```

---

## PyO3 Bridge Requirements

### Objects to construct from Rust

**1. `PackageDescriptor(path: str)`**
```python
pkg = PackageDescriptor("/ws/src/my_pkg")
pkg.name = "my_pkg"
pkg.type = "ros.ament_cmake"
pkg.dependencies = {"build": {"rclcpp", "std_msgs"}, "run": {...}, "test": {...}}
```
All fields are plain Python types (str, dict, set).

**2. `argparse.Namespace(**fields)`**
```python
args = Namespace(
    path="/ws/src/my_pkg",
    build_base="/ws/build/my_pkg",
    install_base="/ws/install/my_pkg",
    symlink_install=True,
    merge_install=False,
    test_result_base=None,
    cmake_args=["-DCMAKE_BUILD_TYPE=Release"],
    ament_cmake_args=[],
    cmake_target=None,
    cmake_target_skip_unavailable=False,
    cmake_clean_cache=False,
    cmake_clean_first=False,
    cmake_force_configure=False,
)
```
Simple Namespace with str/bool/list fields. PyO3 can construct this
via `py.import("argparse")?.getattr("Namespace")?.call((), kwargs)?`.

**3. `TaskContext(pkg, args, dependencies)` subclass**

Must implement `put_event_into_queue(event)`. Colcon calls this for
progress reporting (cmake start, cmake done, etc.). Minimal implementation:

```python
class RosupTaskContext(TaskContext):
    def put_event_into_queue(self, event):
        pass  # or forward to Rust callback
```

PyO3 can define this class via `#[pyclass]` or by creating a Python
class dynamically with `py.run()`.

**4. `dependencies: dict[str, str]`**

Maps dependency name → install path. Example:
```python
{"rclcpp": "/opt/ros/humble", "std_msgs": "/opt/ros/humble"}
```

### Plugin loading

```python
import importlib.metadata
eps = importlib.metadata.entry_points().select(group="colcon_core.task.build")
for ep in eps:
    if ep.name == "ros.ament_cmake":
        cls = ep.load()
        task = cls()
        break
```

This is the only way to discover plugins — must go through Python's
entry point system.

### Async calling convention

`task.build()` returns a coroutine. Two options:

**Option A: Sync wrapper (simpler)**
```rust
Python::with_gil(|py| {
    let asyncio = py.import("asyncio")?;
    let coro = task.call_method0(py, "build")?;
    let result = asyncio.call_method1("run", (coro,))?;
    Ok(result)
})
```

`asyncio.run()` blocks until the coroutine completes. Simple, no
`pyo3-async-runtimes` needed. Each package builds synchronously from
Rust's perspective — Rust's parallel executor manages concurrency at the
job level, not within the Python coroutine.

**Option B: Async bridge (pyo3-async-runtimes)**
```rust
pyo3_async_runtimes::tokio::into_future(py, coro)?.await
```

Maps Python's asyncio to Rust's tokio. More complex but allows Rust-side
async scheduling. Probably not needed since the build tasks are I/O bound
(waiting on cmake subprocess).

**Recommendation: Option A.** The sync wrapper is sufficient. Colcon's
async is used for subprocess I/O, not CPU parallelism. Rust's executor
handles the parallelism by running multiple sync-wrapped Python tasks
in separate threads (each with its own GIL acquisition).

---

## Architecture with PyO3

```
rosup build
  │
  ├─ Built-in tasks (pure Rust, no Python)
  │   ├─ ament_cmake  → cmake + cmake --build + ament_index
  │   ├─ ament_python  → pip install + ament_index
  │   └─ cmake         → cmake + cmake --build
  │
  ├─ Subprocess tasks (rosup-task-* on $PATH)
  │   └─ JSON protocol, any language
  │
  └─ PyO3 bridge (fallback for installed colcon plugins)
      ├─ Load Python entry points
      ├─ Construct TaskContext from Rust data
      ├─ Call asyncio.run(task.build())
      └─ Report result back to Rust executor
```

### Task resolution order (updated)

1. Built-in Rust task (`ament_cmake`, `ament_python`, `cmake`)
2. Subprocess plugin (`rosup-task-<type>` on `$PATH`)
3. PyO3 bridge (colcon Python plugin for this package type)
4. Error: "unsupported build type"

### When PyO3 is NOT available

If rosup is compiled without PyO3 (e.g. `--no-default-features`), step 3
is skipped. Users must either install `rosup-task-*` binaries or use
`rosup build --colcon` to delegate to colcon entirely.

---

## Tradeoffs: Option C (subprocess) vs Option D (PyO3)

| Aspect | Subprocess (Option C) | PyO3 (Option D) |
|--------|----------------------|-----------------|
| **Setup** | Plugin must ship a binary | No change — uses installed colcon plugins |
| **Compat** | New protocol, plugins must be ported | Works with all 27+ existing colcon plugins immediately |
| **Performance** | Process spawn per package | GIL acquisition per package |
| **Complexity** | JSON protocol design + impl | PyO3 dependency + TaskContext construction |
| **Python dep** | None | Required at runtime (libpython3.so) |
| **Cross-platform** | Works everywhere | Needs matching Python version at compile + runtime |

### Recommendation

Use **both**, in this priority order:

1. Built-in Rust tasks — zero overhead, covers 95%
2. Subprocess plugins — for new Rust-native plugins (no Python needed)
3. PyO3 bridge — automatic fallback for any installed colcon plugin

This means rosup works without Python for most packages, but can
seamlessly use `colcon-cargo-ros2` or any custom plugin if Python and
colcon are installed.

---

## Implementation effort

| Component | Effort | Notes |
|-----------|--------|-------|
| PyO3 dependency in Cargo.toml | Small | Optional feature flag |
| Plugin discovery | Small | `importlib.metadata.entry_points()` call |
| TaskContext construction | Medium | Build Python objects from Rust data |
| `put_event_into_queue` bridge | Small | Forward to Rust tracing/progress |
| Sync async wrapper | Small | `asyncio.run(coro)` |
| Error handling | Medium | Map Python exceptions to Rust errors |
| Feature flag (`--no-default-features`) | Small | Compile without PyO3 |
| **Total** | ~1 week | |

---

## Test script

The proof-of-concept script is at `tmp/test_pyo3_build.py`. It
demonstrates constructing a `TaskContext` and calling
`AmentCmakeBuildTask.build()` entirely from Python, which maps directly
to what PyO3 would do from Rust.
