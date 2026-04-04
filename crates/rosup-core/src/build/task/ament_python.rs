//! Build task for `ament_python` packages.
//!
//! Steps:
//! 1. Install Python package via `pip install` into the install prefix.
//!    Uses `--editable` when `symlink_install` is true.
//! 2. Ament layer (pip doesn't generate these unlike cmake --install):
//!    - Copy `package.xml` to `share/<name>/package.xml`
//!    - Create ament index marker
//!    - Generate environment scripts (PYTHONPATH, PATH, etc.)
//! 3. Colcon layer via `post_install`: hook/ dir + colcon-core/packages/

use tracing::info;

use super::{BuildContext, BuildTask, TaskError, run_command};
use crate::build::{ament_index, environment, post_install};

pub struct AmentPythonBuildTask;

impl BuildTask for AmentPythonBuildTask {
    fn build(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        std::fs::create_dir_all(&ctx.install_base).map_err(|e| TaskError::Io {
            cmd: "mkdir install_base".into(),
            source: e,
        })?;

        // Phase 1: install Python package
        info!("{}: pip install", ctx.pkg_name);
        self.pip_install(ctx)?;

        // Phase 2: ament layer (pip doesn't generate these, unlike cmake --install)
        self.install_package_xml(ctx)?;
        self.ensure_ament_index(ctx)?;
        self.generate_environment_scripts(ctx)?;

        // Phase 3: colcon layer (hook/ dir + colcon-core/packages/)
        let ament_prefix = ctx.env.get("AMENT_PREFIX_PATH").map(String::as_str);
        post_install::post_install(
            &ctx.install_base,
            &ctx.pkg_name,
            &ctx.runtime_deps,
            ament_prefix,
        )
        .map_err(|e| TaskError::Other(e.to_string()))?;

        Ok(())
    }

    fn test(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        // ament_python packages use pytest
        let install_str = ctx.install_base.display().to_string();
        run_command(
            "python3",
            &["-m", "pytest", "--tb=short"],
            &ctx.source_path,
            &{
                let mut env = ctx.env.clone();
                // Add the install's Python path so the package can be imported
                let pyver = ctx.python_version.as_deref().unwrap_or("3.10");
                let pythonpath = format!("{}/local/lib/python{pyver}/dist-packages", install_str);
                env.entry("PYTHONPATH".into())
                    .and_modify(|v| *v = format!("{pythonpath}:{v}"))
                    .or_insert(pythonpath);
                env
            },
        )
    }
}

impl AmentPythonBuildTask {
    fn pip_install(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        let install_str = ctx.install_base.display().to_string();

        let mut args = vec![
            "-m",
            "pip",
            "install",
            "--no-deps",
            "--prefix",
            &install_str,
        ];

        if ctx.symlink_install {
            // --editable makes pip create .egg-link files instead of copying
            args.push("--editable");
        }

        args.push(".");

        run_command("python3", &args, &ctx.source_path, &ctx.env)
    }

    fn install_package_xml(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        let src = ctx.source_path.join("package.xml");
        if !src.exists() {
            return Ok(());
        }
        let dst_dir = ctx.install_base.join("share").join(&ctx.pkg_name);
        std::fs::create_dir_all(&dst_dir).map_err(|e| TaskError::Io {
            cmd: "mkdir share/<name>".into(),
            source: e,
        })?;
        if ctx.symlink_install {
            // Symlink instead of copy
            let dst = dst_dir.join("package.xml");
            if dst.exists() {
                std::fs::remove_file(&dst).ok();
            }
            std::os::unix::fs::symlink(&src, &dst).map_err(|e| TaskError::Io {
                cmd: format!("symlink package.xml to {}", dst.display()),
                source: e,
            })?;
        } else {
            std::fs::copy(&src, dst_dir.join("package.xml")).map_err(|e| TaskError::Io {
                cmd: format!("copy package.xml to {}", dst_dir.display()),
                source: e,
            })?;
        }
        Ok(())
    }

    fn ensure_ament_index(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        // The package's setup.py data_files may already install the marker.
        // Write it unconditionally — it's idempotent.
        ament_index::write_package_marker(&ctx.install_base, &ctx.pkg_name)
            .map_err(|e| TaskError::Other(e.to_string()))
    }

    fn generate_environment_scripts(&self, ctx: &BuildContext) -> Result<(), TaskError> {
        // ament_python packages need the PYTHONPATH hook in addition to standard hooks
        let python_version = ctx.python_version.as_deref().unwrap_or("3.10");

        environment::generate_environment_scripts(
            &ctx.install_base,
            &ctx.pkg_name,
            Some(python_version),
        )
        .map_err(|e| TaskError::Other(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_minimal_python_package(dir: &Path) {
        // Minimal setup.py
        std::fs::write(
            dir.join("setup.py"),
            "from setuptools import setup\n\
             setup(\n\
                 name='test_py_pkg',\n\
                 version='0.0.0',\n\
                 packages=['test_py_pkg'],\n\
             )\n",
        )
        .unwrap();

        // Python module
        let pkg_dir = dir.join("test_py_pkg");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("__init__.py"), "# test package\n").unwrap();

        // package.xml
        std::fs::write(
            dir.join("package.xml"),
            "<?xml version=\"1.0\"?>\n\
             <package format=\"3\">\n\
               <name>test_py_pkg</name>\n\
               <version>0.0.0</version>\n\
               <description>test</description>\n\
               <maintainer email=\"a@b.c\">a</maintainer>\n\
               <license>MIT</license>\n\
               <buildtool_depend>ament_python</buildtool_depend>\n\
               <export>\n\
                 <build_type>ament_python</build_type>\n\
               </export>\n\
             </package>\n",
        )
        .unwrap();
    }

    fn make_context(source: &Path, install: &Path) -> BuildContext {
        let mut env = HashMap::new();
        env.insert("PATH".into(), std::env::var("PATH").unwrap_or_default());
        if Path::new("/opt/ros/humble").exists() {
            env.insert("AMENT_PREFIX_PATH".into(), "/opt/ros/humble".into());
        }
        BuildContext {
            pkg_name: "test_py_pkg".into(),
            source_path: source.to_owned(),
            build_base: source.join("build"),
            install_base: install.to_owned(),
            symlink_install: false,
            cmake_args: vec![],
            parallel_jobs: None,
            dep_install_paths: vec![],
            env,
            python_version: Some("3.10".into()),
            runtime_deps: vec![],
        }
    }

    /// Integration test: build a minimal ament_python package.
    /// Requires pip/python3 to be installed.
    #[test]
    #[ignore]
    fn build_minimal_ament_python_package() {
        let tmp = TempDir::new().unwrap();
        let source = tmp.path().join("src");
        let install = tmp.path().join("install");
        std::fs::create_dir_all(&source).unwrap();

        make_minimal_python_package(&source);
        let ctx = make_context(&source, &install);

        let task = AmentPythonBuildTask;
        task.build(&ctx).unwrap();

        // Verify install layout
        assert!(
            install
                .join("share/ament_index/resource_index/packages/test_py_pkg")
                .exists(),
            "ament index marker"
        );
        assert!(
            install.join("share/test_py_pkg/package.xml").exists(),
            "package.xml installed"
        );
        assert!(
            install.join("share/test_py_pkg/local_setup.bash").exists(),
            "local_setup.bash"
        );
        // PYTHONPATH hook should exist
        assert!(
            install
                .join("share/test_py_pkg/environment/pythonpath.dsv")
                .exists(),
            "pythonpath.dsv"
        );
        let dsv =
            std::fs::read_to_string(install.join("share/test_py_pkg/environment/pythonpath.dsv"))
                .unwrap();
        assert!(dsv.contains("PYTHONPATH"));
        assert!(dsv.contains("python3.10"));
    }
}
