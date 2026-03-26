# Phase 12 — Resolver Improvements

Three independent work streams targeting the source pull pipeline, binary
dependency installation, and package.xml condition parsing.

---

## 12.1 Git sparse-checkout for source dependencies

**Current state:** Source deps use a two-level model — bare clone in
`~/.rosup/src/<repo>.git`, worktree in `<project>/.rosup/src/<repo>/`. This
clones the full tree for every source dependency.

**Goal:** Use git sparse-checkout so only the needed package directories are
checked out. Multi-package repos (e.g. `common_interfaces` with 10+ packages)
should only materialise the directories rosup actually needs.

**Files:** `crates/rosup-core/src/resolver/mod.rs` (source pull logic),
new `crates/rosup-core/src/git/sparse.rs`

### 12.1.1 Sparse-checkout primitives — DONE

**Files:** `crates/rosup-core/src/git/mod.rs`, `crates/rosup-core/src/git/sparse.rs`

Uses the low-level sparse-checkout mechanism (`core.sparseCheckout` config +
`info/sparse-checkout` file + `git read-tree -mu HEAD`) rather than the `git
sparse-checkout` porcelain, because the porcelain doesn't work with worktrees
from bare repos on git < 2.38.

- [x] `partial_bare_clone(url, bare_path)` — bare clone with
  `--filter=blob:none` (partial clone). No-op if bare repo already exists.
- [x] `shallow_bare_clone(url, bare_path)` — bare clone with
  `--filter=blob:none --depth=1`. For CI.
- [x] `sparse_worktree_add(bare_path, worktree_path, target)` — add
  worktree with `--no-checkout --detach`.
- [x] `sparse_init_and_checkout(worktree_path, paths)` — enable sparse
  checkout via git config, write patterns file, materialise with
  `git read-tree -mu HEAD`.
- [x] `sparse_add_paths(worktree_path, paths)` — additive merge of new
  paths into existing sparse-checkout file + re-read.
- [x] `sparse_list(worktree_path)` — read configured sparse paths.
- [x] `is_sparse(worktree_path)` — check if sparse-checkout is enabled.
- [x] 9 unit tests with a local bare repo (no network): partial clone,
  idempotency, shallow clone, no-checkout worktree, sparse init, additive
  add, list, empty paths noop, is_sparse detection.

### 12.1.2 Integrate into resolver — DONE

**File:** `crates/rosup-core/src/resolver/mod.rs`

- [x] `execute()` collects package subdirectory paths per repo. Multi-package
  repos (repo name != package name) accumulate sparse paths; single-package
  repos use full checkout.
- [x] `source_pull()` accepts `sparse_paths: &[String]` and `shallow: bool`.
- [x] New worktrees for multi-package repos use sparse checkout (no-checkout +
  init + read-tree). Single-package repos use existing full checkout.
- [x] Existing worktrees with sparse checkout get new paths added additively.
- [x] Partial bare clone (`--filter=blob:none`) is the default for all repos.
  Shallow bare clone (`--depth=1`) only when `--shallow` flag is set.

### 12.1.3 Shallow clone support for CI — DONE

**File:** `crates/rosup-cli/src/main.rs`

- [x] `--shallow` flag on `rosup resolve` — passes `--depth=1` to the
  initial bare clone.
- [x] Threaded through `Resolver::resolve()` → `execute()` → `source_pull()`.

### Acceptance criteria

- [x] Existing integration tests pass unchanged (223 tests, 0 failures).
- [x] `just ci` passes.
- [ ] Manual test: `rosup resolve` on a workspace that pulls
  `common_interfaces` only checks out the needed subdirectory.
- [ ] Manual test: `du -sh .rosup/src/common_interfaces` is significantly
  smaller than a full checkout.

---

## 12.2 Explore `rosdep resolve` for dependency queries

**Current state:** rosup already uses `rosdep resolve <dep> --rosdistro <distro>`
in `resolver/rosdep.rs` to query whether a dependency is available as a
binary. It also uses `rosdep install --from-paths` with a temporary
`package.xml` to install binary deps.

**Goal:** Study whether `rosdep resolve` can replace or improve the
`rosdep install` path, and document findings for future work.

**File:** `crates/rosup-core/src/resolver/rosdep.rs`

### 12.2.1 Research: rosdep resolve vs rosdep install

- [ ] Document the behavioural differences between `rosdep resolve` and
  `rosdep install` — particularly around error handling, key types (ROS
  package names vs. rosdep keys), and installer selection.
- [ ] Evaluate whether using `rosdep resolve` exclusively and calling
  installers directly gives rosup more control (batching, error recovery,
  dry-run output).
- [ ] Test `rosdep resolve` against edge cases:
  - Pure system keys (e.g. `asio`, `libpcl-dev`) that are not ROS packages.
  - Keys that resolve to multiple packages.
  - Keys with no rosdep rule (expected error).
  - Pip-only dependencies.
- [ ] Write findings to `docs/design/rosdep-resolve.md`.

### 12.2.2 Implement direct installer invocation (optional)

If 12.2.1 concludes that direct invocation is better:

- [ ] After `rosdep resolve`, group resolved packages by installer.
- [ ] Call `apt-get install -y <pkgs>` / `pip install <pkgs>` directly
  instead of going through `rosdep install`.
- [ ] Preserve dry-run support (print commands without executing).
- [ ] Better error messages: show which rosdep key maps to which system
  package when installation fails.

### Acceptance criteria

- [ ] Design doc written with clear recommendation.
- [ ] If 12.2.2 is implemented: existing `rosup resolve` integration tests
  pass, and `rosup resolve --dry-run` shows the installer commands.
- [ ] `just ci` passes.

---

## 12.3 Full REP-149 conditional expression parser

**Current state:** `eval_condition()` in `package_xml.rs` handles only simple
binary comparisons (`$ROS_VERSION == 2`). Complex expressions with `and`,
`or`, and parentheses fall through to the conservative default (include the
dep). This is safe but overly broad — real-world package.xml files use
compound conditions.

**Goal:** Implement the full REP-149 condition grammar so compound expressions
are evaluated correctly.

**REP-149 grammar** (from the spec):

```
expr     := or_expr
or_expr  := and_expr ('or' and_expr)*
and_expr := atom ('and' atom)*
atom     := comparison | '(' expr ')'
comparison := value op value
op       := '==' | '!=' | '<' | '<=' | '>' | '>='
value    := '$' IDENT | QUOTED_STRING | UNQUOTED_STRING
```

Variables: `$ROS_VERSION`, `$ROS_DISTRO`, `$ROS_PYTHON_VERSION`, and any
other `$IDENT`. Only `$ROS_VERSION` is substituted with `2`; unresolved
variables cause the expression to evaluate conservatively to `true`.

**File:** `crates/rosup-core/src/package_xml.rs` (replace `eval_condition`)

### 12.3.1 Tokeniser and parser

- [ ] Define token types: `LParen`, `RParen`, `And`, `Or`, `Op(CmpOp)`,
  `Var(String)`, `Literal(String)`.
- [ ] Tokenise the condition string. Quoted strings preserve their content;
  unquoted strings are delimited by whitespace and operators.
- [ ] Recursive-descent parser producing an AST:
  ```rust
  enum Expr {
      Compare { lhs: Value, op: CmpOp, rhs: Value },
      And(Vec<Expr>),
      Or(Vec<Expr>),
  }
  enum Value { Var(String), Literal(String) }
  enum CmpOp { Eq, Ne, Lt, Le, Gt, Ge }
  ```
- [ ] Parse errors fall back to `true` (conservative, same as current
  behaviour — never silently drop a dep).

### 12.3.2 Evaluator

- [ ] Substitute known variables: `$ROS_VERSION` → `"2"`.
  Optionally accept additional variables from caller (for future
  `$ROS_DISTRO` support).
- [ ] If any variable in the expression is unresolved after substitution,
  return `true` (conservative).
- [ ] Evaluate `And`/`Or`/`Compare` nodes. String comparison for all
  operators (REP-149 does not define numeric semantics).

### 12.3.3 Test fixtures from official sources

REP-149 condition examples appear in the official ROS packaging test suites.
Copy or adapt fixtures from:

- `ros-infrastructure/catkin_pkg` — the reference parser for `package.xml`.
  Contains `test/test_condition.py` with condition evaluation tests.
- Real-world `package.xml` files in `ros2/common_interfaces`,
  `ros2/geometry2`, and `ros/catkin` that use compound conditions.

Fixture files to create:

- [ ] `tests/fixtures/package_xml/condition_and_or.xml` — compound `and`/`or`
  conditions mixing ROS 1 and ROS 2 deps.
- [ ] `tests/fixtures/package_xml/condition_parens.xml` — parenthesised
  grouping with nested logic.
- [ ] `tests/fixtures/package_xml/condition_ros_distro.xml` — conditions on
  `$ROS_DISTRO` (unresolved variable, should include deps conservatively).
- [ ] `tests/fixtures/package_xml/condition_quoted.xml` — quoted string
  values in comparisons.
- [ ] `tests/fixtures/package_xml/condition_malformed.xml` — unparseable
  conditions (should fall back to including the dep).

### 12.3.4 Unit tests

- [ ] Tokeniser tests: all token types, edge cases (adjacent operators,
  empty input, only whitespace).
- [ ] Parser tests: simple comparison, `and` chain, `or` chain, mixed
  `and`/`or` (precedence: `and` binds tighter than `or`), nested parens,
  malformed input falls back to `true`.
- [ ] Evaluator tests: `$ROS_VERSION == 2` → true, `$ROS_VERSION == 1` →
  false, `$ROS_VERSION == 2 and $ROS_DISTRO == humble` → true (unresolved
  `$ROS_DISTRO` → conservative), `$ROS_VERSION == 1 or $ROS_VERSION == 2`
  → true.
- [ ] Integration: parse each fixture file, verify correct deps included
  and excluded.
- [ ] Backwards compatibility: existing `conditional_deps.xml` fixture
  produces identical results.

### Acceptance criteria

- [ ] All compound conditions from `catkin_pkg` test suite produce matching
  results.
- [ ] Existing tests pass unchanged (no regressions).
- [ ] `just ci` passes.

---

## Sequencing

These three work streams are independent and can be done in parallel. However,
a suggested priority order:

1. **12.3** (REP-149 parser) — self-contained, improves correctness now,
   no external deps.
2. **12.2** (rosdep research) — research-first, low risk, informs future
   resolver improvements.
3. **12.1** (sparse-checkout) — largest scope, biggest payoff for large
   workspaces, benefits from 3.4 fixes (bare clone + worktree rewrite).
