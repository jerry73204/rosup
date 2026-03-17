# package.xml Reference (Format 3, ROS 2)

ROS 2 uses package.xml **format 3** (REP 149). This is the source of truth for
package metadata and dependencies. rox reads and writes this file; it does not
replace it.

---

## Document Structure

```xml
<?xml version="1.0"?>
<?xml-model href="http://download.ros.org/schema/package_format3.xsd"
            schematypens="http://www.w3.org/2001/XMLSchema"?>
<package format="3">
  ...
</package>
```

- The `<?xml-model?>` processing instruction is conventional and references the
  XSD schema for editor validation. It is not required by the parser.
- No DOCTYPE or XML namespaces are used.
- Encoding is UTF-8 by convention.

---

## Required Elements

| Element | Cardinality | Notes |
|---|---|---|
| `<name>` | exactly 1 | Lowercase, alphanumeric + underscores |
| `<version>` | exactly 1 | `MAJOR.MINOR.PATCH`; optional `compatibility` attribute (see below) |
| `<description>` | exactly 1 | Free text; XHTML markup permitted |
| `<maintainer>` | 1 or more | `email` attribute is required |
| `<license>` | 1 or more | `file` attribute optionally points to the license text file |

### `<version>` compatibility attribute

```xml
<version compatibility="1.2.0">1.3.5</version>
```

Signals that the current version (1.3.5) is ABI/API-compatible back to 1.2.0.
Used by packaging tools to avoid unnecessary rebuilds of downstream packages.
If omitted, no backward compatibility is implied.

---

## Optional Metadata Elements

| Element | Cardinality | Attributes | Notes |
|---|---|---|---|
| `<url>` | 0 or more | `type`: `website` (default), `repository`, `bugtracker` | |
| `<author>` | 0 or more | `email` (optional) | Contributors |

---

## Dependency Elements

All dependency tags accept these optional attributes:

| Attribute | Meaning |
|---|---|
| `version_lt` | dep version < value |
| `version_lte` | dep version ≤ value |
| `version_eq` | dep version = value |
| `version_gte` | dep version ≥ value |
| `version_gt` | dep version > value |
| `condition` | conditional inclusion expression (see below) |

Multiple version attributes may be combined to express a range.

### Dependency types

| Tag | Meaning |
|---|---|
| `<depend>` | Shorthand for `build_depend` + `build_export_depend` + `exec_depend`. Most common for typical library deps. |
| `<build_depend>` | Needed at compile time only (e.g., header-only libs, codegen tools). |
| `<build_export_depend>` | Headers/interfaces exported in this package's public API. Creates transitive build dep for consumers. |
| `<buildtool_depend>` | Build system tool run on the build host. Typical: `ament_cmake`, `ament_cmake_ros`. |
| `<buildtool_export_depend>` | Build tools that consumers of this package also need. Relevant for cross-compilation. |
| `<exec_depend>` | Runtime dependency: shared libs, Python modules, scripts, launch files. |
| `<test_depend>` | Required only to build and run tests. Not included in binary packages. |
| `<doc_depend>` | Required only for documentation generation (e.g., `doxygen`). Not included in binary packages. |
| `<conflict>` | Declares incompatibility with another package. |
| `<replace>` | Declares this package supersedes another. |

### `<group_depend>` and `<member_of_group>`

Introduced in format 3. Enables plugin-style extensibility in source builds.

```xml
<!-- Package that needs some logging backend, but doesn't care which: -->
<group_depend>rcl_logging_packages</group_depend>

<!-- A specific logging backend that satisfies the group: -->
<member_of_group>rcl_logging_packages</member_of_group>
```

The build tool resolves all workspace packages that are members of the group
and treats them as direct dependencies. These are **not** used in binary
packaging (rosdep/apt/bloom ignore them).

Both accept the `condition` attribute.

---

## The `condition` Attribute

Introduced in format 3. Allows dependency elements to be conditionally
included based on environment variables.

### Syntax

```
expression := condition | binop
condition  := term operator term
binop      := expression 'and' expression | expression 'or' expression
operator   := '==' | '!=' | '>=' | '>' | '<=' | '<'
term       := $VARIABLE | "literal" | 'literal' | bare_literal
```

- Variables are prefixed with `$`. Unknown variables resolve to `""`.
- Comparisons are **lexicographic strings**, not numeric — use with care on
  version numbers (`"2" > "10"` is `True`).
- Parentheses are **not** supported.

### Standard ROS 2 condition variables

| Variable | Example values | Notes |
|---|---|---|
| `$ROS_VERSION` | `"2"` | Always `"2"` in ROS 2 |
| `$ROS_DISTRO` | `"jazzy"`, `"iron"`, `"rolling"` | Distribution name |
| `$ROS_PYTHON_VERSION` | `"3"` | Always `"3"` in ROS 2 |

Any environment variable is accessible. Missing variables default to `""`.

### Examples

```xml
<!-- Distro-specific dependency -->
<depend condition="$ROS_DISTRO == 'jazzy'">some_jazzy_only_pkg</depend>

<!-- Exclude on a specific distro -->
<test_depend condition="$ROS_DISTRO != 'rolling'">deprecated_linter</test_depend>
```

---

## The `<export>` Element

An open extension container. All child elements are preserved as-is by
parsers. Tool-specific metadata goes here.

```xml
<export>
  <build_type>ament_cmake</build_type>
</export>
```

### Well-known sub-elements in ROS 2

| Element | Meaning |
|---|---|
| `<build_type>` | Build system: `ament_cmake`, `ament_python`, or `cmake`. Required to be recognised as a ROS 2 package. Accepts `condition` attribute. |
| `<architecture_independent/>` | No architecture-specific artifacts. Generates `noarch` packages. Appropriate for Python-only or message-only packages. |
| `<deprecated>` | Optional deprecation message. |
| `<rosdoc2>rosdoc2.yaml</rosdoc2>` | Path to rosdoc2 configuration file. |
| `<ros1_bridge mapping_rules="..."/>` | Type mapping for the ros1_bridge. |

Custom sub-elements are freely allowed — rox uses `<export>` as its own
extension point where needed.

---

## Parsing Behaviour

- **Whitespace**: All text values are whitespace-normalized (collapsed to single
  space, then stripped). Exception: `<description>` preserves intentional blank
  lines.
- **Element order**: Not significant. Elements may appear in any order.
- **Comments**: Preserved in the XML file; ignored by parsers.
- **CDATA**: Not used in practice; handled transparently by XML parsers.
- **Unknown top-level elements**: Trigger a parser warning/error.
- **Unknown `<export>` sub-elements**: Silently accepted (open extension point).

---

## Canonical Element Order (convention, not enforced)

```
name → version → description → maintainer(s) → license →
url(s) → author(s) →
buildtool_depend → depend → build_depend → build_export_depend →
exec_depend → test_depend → doc_depend →
group_depend → member_of_group → conflict → replace →
export
```

---

## Minimal Valid Package (ament_cmake)

```xml
<?xml version="1.0"?>
<package format="3">
  <name>my_package</name>
  <version>0.1.0</version>
  <description>My ROS 2 package.</description>
  <maintainer email="dev@example.com">Your Name</maintainer>
  <license>Apache-2.0</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
  <export>
    <build_type>ament_cmake</build_type>
  </export>
</package>
```

## Realistic ament_cmake Package

```xml
<?xml version="1.0"?>
<?xml-model href="http://download.ros.org/schema/package_format3.xsd"
            schematypens="http://www.w3.org/2001/XMLSchema"?>
<package format="3">
  <name>my_robot_nav</name>
  <version>1.0.0</version>
  <description>Navigation package for my robot.</description>
  <maintainer email="dev@example.com">Your Name</maintainer>
  <license>Apache-2.0</license>

  <url type="repository">https://github.com/example/my_robot</url>
  <url type="bugtracker">https://github.com/example/my_robot/issues</url>

  <buildtool_depend>ament_cmake</buildtool_depend>

  <depend>rclcpp</depend>
  <depend>nav_msgs</depend>
  <depend>sensor_msgs</depend>

  <test_depend>ament_lint_auto</test_depend>
  <test_depend>ament_lint_common</test_depend>
  <test_depend>ament_cmake_gtest</test_depend>

  <export>
    <build_type>ament_cmake</build_type>
  </export>
</package>
```

## Realistic ament_python Package

```xml
<?xml version="1.0"?>
<?xml-model href="http://download.ros.org/schema/package_format3.xsd"
            schematypens="http://www.w3.org/2001/XMLSchema"?>
<package format="3">
  <name>my_robot_bringup</name>
  <version>1.0.0</version>
  <description>Bringup launch files for my robot.</description>
  <maintainer email="dev@example.com">Your Name</maintainer>
  <license>Apache-2.0</license>

  <exec_depend>rclpy</exec_depend>
  <exec_depend>launch</exec_depend>
  <exec_depend>launch_ros</exec_depend>

  <test_depend>ament_copyright</test_depend>
  <test_depend>ament_flake8</test_depend>
  <test_depend>ament_pep257</test_depend>
  <test_depend>python3-pytest</test_depend>

  <export>
    <build_type>ament_python</build_type>
  </export>
</package>
```

Note: `ament_python` packages have no `<buildtool_depend>` and no
`<build_depend>` / `<build_export_depend>` — only `<exec_depend>` for
runtime libraries.
