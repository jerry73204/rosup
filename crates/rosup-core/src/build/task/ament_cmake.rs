//! Build task for `ament_cmake` packages.
//!
//! Steps:
//! 1. Configure: `cmake <source> -B <build> -DCMAKE_INSTALL_PREFIX=<install> ...`
//! 2. Build: `cmake --build <build> --parallel <jobs>`
//! 3. Install: `cmake --install <build>`
//! 4. Write ament index + environment scripts

use tracing::info;

use super::{BuildContext, BuildTask, TaskError, run_command};
use crate::build::{ament_index, environment};

pub struct AmentCmakeBuildTask;

impl BuildTask for AmentCmakeBuildTask {
    fn build(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        std::fs::create_dir_all(&ctx.build_base).map_err(|e| TaskError::Io {
            cmd: "mkdir build_base".into(),
            source: e,
        })?;
        std::fs::create_dir_all(&ctx.install_base).map_err(|e| TaskError::Io {
            cmd: "mkdir install_base".into(),
            source: e,
        })?;

        // Phase 1: configure (skip if CMakeCache.txt exists for incremental builds)
        let needs_configure = !ctx.build_base.join("CMakeCache.txt").exists();
        if needs_configure {
            info!("{}: cmake configure", ctx.pkg_name);
            self.configure(ctx)?;
        }

        // Phase 2: build
        info!("{}: cmake build", ctx.pkg_name);
        self.cmake_build(ctx)?;

        // Phase 3: install
        info!("{}: cmake install", ctx.pkg_name);
        self.cmake_install(ctx)?;

        // Phase 4: copy package.xml to install
        self.install_package_xml(ctx)?;

        // Phase 5: ament index + environment scripts
        self.write_metadata(ctx)?;

        Ok(())
    }

    fn test(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        // Run ctest from the build directory
        let mut args = vec!["--test-dir", ctx.build_base.to_str().unwrap_or(".")];
        args.push("--output-on-failure");
        if let Some(jobs) = ctx.parallel_jobs {
            let jobs_str = jobs.to_string();
            // ctest -j <n>
            run_command(
                "ctest",
                &[
                    "--test-dir",
                    ctx.build_base.to_str().unwrap_or("."),
                    "--output-on-failure",
                    "-j",
                    &jobs_str,
                ],
                &ctx.build_base,
                &ctx.env,
            )
        } else {
            run_command("ctest", &args, &ctx.build_base, &ctx.env)
        }
    }
}

impl AmentCmakeBuildTask {
    fn configure(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        let source = ctx.source_path.display().to_string();
        let install_prefix = format!("-DCMAKE_INSTALL_PREFIX={}", ctx.install_base.display());

        let mut args: Vec<String> = vec![
            source,
            format!("-B{}", ctx.build_base.display()),
            install_prefix,
        ];

        // CMAKE_PREFIX_PATH from resolved deps
        if !ctx.dep_install_paths.is_empty() {
            let prefix_path: String = ctx
                .dep_install_paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(";");
            args.push(format!("-DCMAKE_PREFIX_PATH={prefix_path}"));
        }

        // Symlink install
        if ctx.symlink_install {
            args.push("-DAMENT_CMAKE_SYMLINK_INSTALL=1".into());
        }

        // Disable tests during build (colcon convention)
        args.push("-DBUILD_TESTING=OFF".into());

        // User cmake args
        args.extend(ctx.cmake_args.iter().cloned());

        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_command("cmake", &arg_refs, &ctx.source_path, &ctx.env)
    }

    fn cmake_build(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        let mut args = vec!["--build", ctx.build_base.to_str().unwrap_or(".")];
        let jobs_str;
        if let Some(jobs) = ctx.parallel_jobs {
            jobs_str = jobs.to_string();
            args.push("--parallel");
            args.push(&jobs_str);
        }
        run_command("cmake", &args, &ctx.source_path, &ctx.env)
    }

    fn cmake_install(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        run_command(
            "cmake",
            &["--install", ctx.build_base.to_str().unwrap_or(".")],
            &ctx.source_path,
            &ctx.env,
        )
    }

    fn install_package_xml(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        let src = ctx.source_path.join("package.xml");
        let dst_dir = ctx.install_base.join("share").join(&ctx.pkg_name);
        std::fs::create_dir_all(&dst_dir).map_err(|e| TaskError::Io {
            cmd: "mkdir share/<name>".into(),
            source: e,
        })?;
        std::fs::copy(&src, dst_dir.join("package.xml")).map_err(|e| TaskError::Io {
            cmd: format!("copy package.xml to {}", dst_dir.display()),
            source: e,
        })?;
        Ok(())
    }

    fn write_metadata(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        ament_index::write_package_marker(&ctx.install_base, &ctx.pkg_name)
            .map_err(|e| TaskError::Other(e.to_string()))?;

        ament_index::write_runtime_deps(&ctx.install_base, &ctx.pkg_name, &ctx.runtime_deps)
            .map_err(|e| TaskError::Other(e.to_string()))?;

        environment::generate_environment_scripts(
            &ctx.install_base,
            &ctx.pkg_name,
            ctx.python_version.as_deref(),
        )
        .map_err(|e| TaskError::Other(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_minimal_cmake_package(dir: &Path) {
        std::fs::write(
            dir.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.8)\n\
             project(test_pkg)\n\
             find_package(ament_cmake REQUIRED)\n\
             ament_package()\n",
        )
        .unwrap();
        std::fs::write(
            dir.join("package.xml"),
            "<?xml version=\"1.0\"?>\n\
             <package format=\"3\">\n\
               <name>test_pkg</name>\n\
               <version>0.0.0</version>\n\
               <description>test</description>\n\
               <maintainer email=\"a@b.c\">a</maintainer>\n\
               <license>MIT</license>\n\
               <buildtool_depend>ament_cmake</buildtool_depend>\n\
             </package>\n",
        )
        .unwrap();
    }

    fn make_context(source: &Path, build: &Path, install: &Path) -> BuildContext {
        let mut env = HashMap::new();
        // Need ament_cmake on the prefix path
        if Path::new("/opt/ros/humble").exists() {
            env.insert("CMAKE_PREFIX_PATH".into(), "/opt/ros/humble".into());
            env.insert("AMENT_PREFIX_PATH".into(), "/opt/ros/humble".into());
            env.insert(
                "PATH".into(),
                format!(
                    "/opt/ros/humble/bin:{}",
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
        BuildContext {
            pkg_name: "test_pkg".into(),
            source_path: source.to_owned(),
            build_base: build.to_owned(),
            install_base: install.to_owned(),
            symlink_install: true,
            cmake_args: vec![],
            parallel_jobs: Some(4),
            dep_install_paths: vec![],
            env,
            python_version: None,
            runtime_deps: vec![],
        }
    }

    /// Integration test: build a minimal ament_cmake package.
    /// Requires a fully sourced ROS 2 environment.
    /// Run with: `cargo test --lib -- --ignored build_minimal`
    #[test]
    #[ignore]
    fn build_minimal_ament_cmake_package() {
        if !Path::new("/opt/ros/humble/share/ament_cmake").exists() {
            eprintln!("skipping: ament_cmake not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("src");
        let build = tmp.path().join("build");
        let install = tmp.path().join("install");
        std::fs::create_dir_all(&source).unwrap();

        make_minimal_cmake_package(&source);
        let ctx = make_context(&source, &build, &install);

        let task = AmentCmakeBuildTask;
        task.build(&ctx).unwrap();

        // Verify install layout
        assert!(
            install
                .join("share/ament_index/resource_index/packages/test_pkg")
                .exists(),
            "ament index marker"
        );
        assert!(
            install.join("share/test_pkg/package.xml").exists(),
            "package.xml installed"
        );
        assert!(
            install.join("share/test_pkg/local_setup.bash").exists(),
            "local_setup.bash"
        );
        assert!(
            install
                .join("share/test_pkg/environment/ament_prefix_path.dsv")
                .exists(),
            "ament_prefix_path.dsv"
        );
        assert!(
            install.join("share/colcon-core/packages/test_pkg").exists(),
            "colcon runtime deps"
        );
    }

    /// Verify incremental build skips configure when CMakeCache.txt exists.
    /// Requires a fully sourced ROS 2 environment.
    #[test]
    #[ignore]
    fn incremental_build_skips_configure() {
        if !Path::new("/opt/ros/humble/share/ament_cmake").exists() {
            eprintln!("skipping: ament_cmake not installed");
            return;
        }

        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("src");
        let build = tmp.path().join("build");
        let install = tmp.path().join("install");
        std::fs::create_dir_all(&source).unwrap();

        make_minimal_cmake_package(&source);
        let ctx = make_context(&source, &build, &install);

        let task = AmentCmakeBuildTask;
        // First build: configures + builds
        task.build(&ctx).unwrap();
        let cache_mtime_1 = std::fs::metadata(build.join("CMakeCache.txt"))
            .unwrap()
            .modified()
            .unwrap();

        // Second build: should skip configure
        std::thread::sleep(std::time::Duration::from_millis(100));
        task.build(&ctx).unwrap();
        let cache_mtime_2 = std::fs::metadata(build.join("CMakeCache.txt"))
            .unwrap()
            .modified()
            .unwrap();

        // CMakeCache.txt should not be rewritten
        assert_eq!(cache_mtime_1, cache_mtime_2, "configure should be skipped");
    }
}
