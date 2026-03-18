//! Package scaffolding — `rosup new`.
//!
//! Creates a new ROS 2 package directory with build-system files, a valid
//! `package.xml`, and a `rosup.toml`.

use std::path::{Path, PathBuf};
use thiserror::Error;

// ── BuildType ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildType {
    #[default]
    AmentCmake,
    AmentPython,
}

impl std::str::FromStr for BuildType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ament_cmake" => Ok(Self::AmentCmake),
            "ament_python" => Ok(Self::AmentPython),
            other => Err(format!(
                "unknown build type `{other}`; expected ament_cmake or ament_python"
            )),
        }
    }
}

impl std::fmt::Display for BuildType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AmentCmake => f.write_str("ament_cmake"),
            Self::AmentPython => f.write_str("ament_python"),
        }
    }
}

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum NewError {
    #[error("destination `{0}` already exists")]
    AlreadyExists(PathBuf),
    #[error("failed to create {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

// ── Options ───────────────────────────────────────────────────────────────────

pub struct NewOptions<'a> {
    /// Package name (also the new directory name).
    pub name: &'a str,
    /// Build system to use.
    pub build_type: BuildType,
    /// ROS distribution (e.g. "humble"). Written into `[resolve]` in `rosup.toml`.
    pub ros_distro: &'a str,
    /// Additional `<depend>` entries for `package.xml`.
    pub deps: &'a [String],
    /// If `Some`, generate a starter node file with this name.
    pub node_name: Option<&'a str>,
    /// Parent directory in which `<name>/` is created.
    pub destination: &'a Path,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Scaffold a new ROS 2 package directory and return its path.
///
/// Creates `<destination>/<name>/` with `package.xml`, build-system files, and
/// a `rosup.toml`.  Fails if the directory already exists.
pub fn scaffold(opts: &NewOptions<'_>) -> Result<PathBuf, NewError> {
    let pkg_dir = opts.destination.join(opts.name);
    if pkg_dir.exists() {
        return Err(NewError::AlreadyExists(pkg_dir));
    }

    fs_create_dir(&pkg_dir)?;
    fs_write(&pkg_dir.join("package.xml"), render_package_xml(opts))?;

    match opts.build_type {
        BuildType::AmentCmake => scaffold_ament_cmake(&pkg_dir, opts)?,
        BuildType::AmentPython => scaffold_ament_python(&pkg_dir, opts)?,
    }

    // Write rosup.toml directly — we know it's a single package.
    let overlay_path = format!("/opt/ros/{}", opts.ros_distro);
    fs_write(
        &pkg_dir.join("rosup.toml"),
        format!(
            "[package]\nname = \"{}\"\n\n[resolve]\nros-distro = \"{}\"\noverlays = [\"{overlay_path}\"]\n",
            opts.name, opts.ros_distro
        ),
    )?;

    // Add .rosup/ to .gitignore (new package won't have one yet).
    fs_write(&pkg_dir.join(".gitignore"), ".rosup/\n")?;

    Ok(pkg_dir)
}

// ── ament_cmake ───────────────────────────────────────────────────────────────

fn scaffold_ament_cmake(pkg_dir: &Path, opts: &NewOptions<'_>) -> Result<(), NewError> {
    fs_create_dir(&pkg_dir.join("src"))?;
    fs_create_dir(&pkg_dir.join("include").join(opts.name))?;
    fs_write(&pkg_dir.join("CMakeLists.txt"), render_cmake(opts))?;

    if let Some(node) = opts.node_name {
        fs_write(
            &pkg_dir.join("src").join(format!("{node}.cpp")),
            render_cpp_node(node),
        )?;
    }

    Ok(())
}

fn render_cmake(opts: &NewOptions<'_>) -> String {
    let name = opts.name;

    let mut lines: Vec<String> = vec!["find_package(ament_cmake REQUIRED)".to_owned()];
    for dep in opts.deps {
        lines.push(format!("find_package({dep} REQUIRED)"));
    }

    if let Some(node) = opts.node_name {
        let dep_list = if opts.deps.is_empty() {
            String::new()
        } else {
            format!("\n  {}", opts.deps.join("\n  "))
        };
        lines.push(String::new());
        lines.push(format!(
            "add_executable({node} src/{node}.cpp)\n\
             ament_target_dependencies({node}{dep_list})\n\
             install(TARGETS {node} DESTINATION lib/${{PROJECT_NAME}})"
        ));
    }

    let body = lines.join("\n");

    format!(
        r#"cmake_minimum_required(VERSION 3.8)
project({name})

if(CMAKE_COMPILER_IS_GNUCXX OR CMAKE_CXX_COMPILER_ID MATCHES "Clang")
  add_compile_options(-Wall -Wextra -Wpedantic)
endif()

{body}

if(BUILD_TESTING)
  find_package(ament_lint_auto REQUIRED)
  ament_lint_auto_find_test_dependencies()
endif()

ament_package()
"#
    )
}

fn render_cpp_node(node: &str) -> String {
    let class_name = to_pascal_case(node);
    format!(
        r#"#include "rclcpp/rclcpp.hpp"

class {class_name} : public rclcpp::Node
{{
public:
  {class_name}()
  : Node("{node}")
  {{}}
}};

int main(int argc, char * argv[])
{{
  rclcpp::init(argc, argv);
  rclcpp::spin(std::make_shared<{class_name}>());
  rclcpp::shutdown();
  return 0;
}}
"#
    )
}

// ── ament_python ──────────────────────────────────────────────────────────────

fn scaffold_ament_python(pkg_dir: &Path, opts: &NewOptions<'_>) -> Result<(), NewError> {
    let name = opts.name;

    // resource/<name> — ament resource index marker.
    let resource_dir = pkg_dir.join("resource");
    fs_create_dir(&resource_dir)?;
    fs_write(&resource_dir.join(name), "")?;

    // Python package directory.
    let py_pkg = pkg_dir.join(name);
    fs_create_dir(&py_pkg)?;
    fs_write(&py_pkg.join("__init__.py"), "")?;

    if let Some(node) = opts.node_name {
        fs_write(&py_pkg.join(format!("{node}.py")), render_py_node(node))?;
    }

    fs_write(&pkg_dir.join("setup.py"), render_setup_py(opts))?;
    fs_write(&pkg_dir.join("setup.cfg"), render_setup_cfg(name))?;

    Ok(())
}

fn render_setup_py(opts: &NewOptions<'_>) -> String {
    let name = opts.name;
    let entry_points = match opts.node_name {
        Some(node) => format!("'{node} = {name}.{node}:main',\n        "),
        None => String::new(),
    };
    format!(
        r#"from setuptools import find_packages, setup

package_name = '{name}'

setup(
    name=package_name,
    version='0.0.0',
    packages=find_packages(exclude=['test']),
    data_files=[
        ('share/ament_index/resource_index/packages',
            ['resource/' + package_name]),
        ('share/' + package_name, ['package.xml']),
    ],
    install_requires=['setuptools'],
    zip_safe=True,
    maintainer='user',
    maintainer_email='user@todo.todo',
    description='TODO: Package description',
    license='TODO: License declaration',
    tests_require=['pytest'],
    entry_points={{
        'console_scripts': [
            {entry_points}
        ],
    }},
)
"#
    )
}

fn render_setup_cfg(name: &str) -> String {
    format!("[develop]\nscript_dir=$base/lib/{name}\n[install]\ninstall_scripts=$base/lib/{name}\n")
}

fn render_py_node(node: &str) -> String {
    let class_name = to_pascal_case(node);
    format!(
        r#"import rclpy
from rclpy.node import Node


class {class_name}(Node):
    def __init__(self):
        super().__init__('{node}')


def main(args=None):
    rclpy.init(args=args)
    node = {class_name}()
    rclpy.spin(node)
    node.destroy_node()
    rclpy.shutdown()


if __name__ == '__main__':
    main()
"#
    )
}

// ── package.xml ───────────────────────────────────────────────────────────────

fn render_package_xml(opts: &NewOptions<'_>) -> String {
    let name = opts.name;
    let build_type = opts.build_type;
    let buildtool = build_type.to_string();

    let dep_section = if opts.deps.is_empty() {
        String::new()
    } else {
        let lines: String = opts
            .deps
            .iter()
            .map(|d| format!("  <depend>{d}</depend>\n"))
            .collect();
        format!("\n{lines}")
    };

    let test_deps = match build_type {
        BuildType::AmentCmake => {
            "  <test_depend>ament_lint_auto</test_depend>\n\
               <test_depend>ament_lint_common</test_depend>\n"
        }
        BuildType::AmentPython => {
            "  <test_depend>ament_copyright</test_depend>\n\
               <test_depend>ament_flake8</test_depend>\n\
               <test_depend>ament_pep257</test_depend>\n\
               <test_depend>python3-pytest</test_depend>\n"
        }
    };

    format!(
        r#"<?xml version="1.0"?>
<?xml-model href="http://download.ros.org/schema/package_format3.xsd" schematypens="http://www.w3.org/2001/XMLSchema"?>
<package format="3">
  <name>{name}</name>
  <version>0.0.0</version>
  <description>TODO: Package description</description>
  <maintainer email="user@todo.todo">user</maintainer>
  <license>TODO: License declaration</license>

  <buildtool_depend>{buildtool}</buildtool_depend>{dep_section}
  {test_deps}
  <export>
    <build_type>{build_type}</build_type>
  </export>
</package>
"#
    )
}

// ── utilities ─────────────────────────────────────────────────────────────────

fn fs_create_dir(path: &Path) -> Result<(), NewError> {
    std::fs::create_dir_all(path).map_err(|e| NewError::Io {
        path: path.to_owned(),
        source: e,
    })
}

fn fs_write(path: &Path, content: impl AsRef<[u8]>) -> Result<(), NewError> {
    std::fs::write(path, content).map_err(|e| NewError::Io {
        path: path.to_owned(),
        source: e,
    })
}

/// Convert `snake_case` or `kebab-case` to `PascalCase`.
fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut chars = p.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn opts<'a>(
        name: &'a str,
        build_type: BuildType,
        deps: &'a [String],
        node_name: Option<&'a str>,
        destination: &'a Path,
    ) -> NewOptions<'a> {
        NewOptions {
            name,
            ros_distro: "humble",
            build_type,
            deps,
            node_name,
            destination,
        }
    }

    #[test]
    fn scaffold_ament_cmake_creates_expected_files() {
        let tmp = TempDir::new().unwrap();
        let pkg = scaffold(&opts(
            "my_pkg",
            BuildType::AmentCmake,
            &[],
            None,
            tmp.path(),
        ))
        .unwrap();
        assert!(pkg.join("package.xml").exists());
        assert!(pkg.join("CMakeLists.txt").exists());
        assert!(pkg.join("rosup.toml").exists());
        assert!(pkg.join("src").is_dir());
        assert!(pkg.join("include/my_pkg").is_dir());
        // No node file by default.
        assert!(!pkg.join("src/my_pkg.cpp").exists());
    }

    #[test]
    fn scaffold_ament_python_creates_expected_files() {
        let tmp = TempDir::new().unwrap();
        let pkg = scaffold(&opts(
            "my_py_pkg",
            BuildType::AmentPython,
            &[],
            None,
            tmp.path(),
        ))
        .unwrap();
        assert!(pkg.join("package.xml").exists());
        assert!(pkg.join("setup.py").exists());
        assert!(pkg.join("setup.cfg").exists());
        assert!(pkg.join("resource/my_py_pkg").exists());
        assert!(pkg.join("my_py_pkg/__init__.py").exists());
        assert!(pkg.join("rosup.toml").exists());
    }

    #[test]
    fn scaffold_cmake_with_node_creates_cpp_file() {
        let tmp = TempDir::new().unwrap();
        let pkg = scaffold(&opts(
            "my_pkg",
            BuildType::AmentCmake,
            &[],
            Some("talker"),
            tmp.path(),
        ))
        .unwrap();
        assert!(pkg.join("src/talker.cpp").exists());
        let cmake = std::fs::read_to_string(pkg.join("CMakeLists.txt")).unwrap();
        assert!(cmake.contains("add_executable(talker"));
    }

    #[test]
    fn scaffold_python_with_node_creates_py_file() {
        let tmp = TempDir::new().unwrap();
        let pkg = scaffold(&opts(
            "my_py_pkg",
            BuildType::AmentPython,
            &[],
            Some("listener"),
            tmp.path(),
        ))
        .unwrap();
        assert!(pkg.join("my_py_pkg/listener.py").exists());
        let setup = std::fs::read_to_string(pkg.join("setup.py")).unwrap();
        assert!(setup.contains("listener = my_py_pkg.listener:main"));
    }

    #[test]
    fn scaffold_errors_if_directory_already_exists() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("my_pkg")).unwrap();
        let err = scaffold(&opts(
            "my_pkg",
            BuildType::AmentCmake,
            &[],
            None,
            tmp.path(),
        ))
        .unwrap_err();
        assert!(matches!(err, NewError::AlreadyExists(_)));
    }

    #[test]
    fn package_xml_contains_deps() {
        let tmp = TempDir::new().unwrap();
        let deps = vec!["rclcpp".to_owned(), "std_msgs".to_owned()];
        let pkg = scaffold(&opts(
            "my_pkg",
            BuildType::AmentCmake,
            &deps,
            None,
            tmp.path(),
        ))
        .unwrap();
        let xml = std::fs::read_to_string(pkg.join("package.xml")).unwrap();
        assert!(xml.contains("<depend>rclcpp</depend>"));
        assert!(xml.contains("<depend>std_msgs</depend>"));
    }

    #[test]
    fn rosup_toml_has_package_and_resolve_sections() {
        let tmp = TempDir::new().unwrap();
        let pkg = scaffold(&opts(
            "my_pkg",
            BuildType::AmentCmake,
            &[],
            None,
            tmp.path(),
        ))
        .unwrap();
        let toml = std::fs::read_to_string(pkg.join("rosup.toml")).unwrap();
        assert!(toml.contains("[package]"));
        assert!(toml.contains("name = \"my_pkg\""));
        assert!(toml.contains("[resolve]"));
        assert!(toml.contains("ros-distro = \"humble\""));
        assert!(toml.contains("/opt/ros/humble"));
    }

    #[test]
    fn to_pascal_case_converts_correctly() {
        assert_eq!(to_pascal_case("my_node"), "MyNode");
        assert_eq!(to_pascal_case("talker"), "Talker");
        assert_eq!(to_pascal_case("my-node"), "MyNode");
        assert_eq!(to_pascal_case(""), "");
    }
}
