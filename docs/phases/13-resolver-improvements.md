# Phase 13 — Resolver Improvements

Three independent work streams targeting the source pull pipeline, binary
dependency installation, and package.xml condition parsing.

---

## 13.1 Git sparse-checkout for source dependencies

**Current state:** Source deps use a two-level model — bare clone in
`~/.rosup/src/<repo>.git`, worktree in `<project>/.rosup/src/<repo>/`. This
clones the full tree for every source dependency.

**Goal:** Use git sparse-checkout so only the needed package directories are
checked out. Multi-package repos (e.g. `common_interfaces` with 10+ packages)
should only materialise the directories rosup actually needs.

**Files:** `crates/rosup-core/src/resolver/mod.rs` (source pull logic),
new `crates/rosup-core/src/git/sparse.rs`

### 13.1.1 Sparse-checkout primitives — DONE

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

### 13.1.2 Integrate into resolver — DONE

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

### 13.1.3 Shallow clone support for CI — DONE

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

## 13.2 Direct installer invocation via `rosdep resolve` — DONE

**Previous state:** rosup used `rosdep install --from-paths` with a
temporary `package.xml` to install binary deps. This required writing a
fake package.xml and gave opaque error messages.

**New approach:** Use `rosdep resolve` exclusively to map keys to system
packages, then call `apt-get install` / `pip install` directly.

See `docs/design/rosdep-resolve.md` for the full analysis.

**File:** `crates/rosup-core/src/resolver/rosdep.rs`

### 13.2.1 Research: rosdep resolve vs rosdep install — DONE

- [x] Documented behavioural differences between `rosdep resolve` and
  `rosdep install` in `docs/design/rosdep-resolve.md`.
- [x] Evaluated direct invocation: better error messages, no temp files,
  per-key failure handling, installer grouping.
- [x] Edge cases verified: pure system keys, multi-package keys, missing
  rules, pip dependencies.
- [x] Design doc written with recommendation: use `rosdep resolve`
  exclusively, call installers directly.

### 13.2.2 Implement direct installer invocation — DONE

- [x] `resolve_all(keys, distro)` — batch-resolve all keys, group by
  installer, collect failures separately.
- [x] `install_direct(resolved, dry_run, yes)` — call `apt-get install` /
  `pip install` directly from grouped results.
- [x] `ResolveAllResult` and `InstalledPackage` structs for traceability
  (rosdep key → system package mapping).
- [x] Resolver `execute()` now uses `resolve_all()` + `install_direct()`
  instead of `rosdep::install()`.
- [x] Old `rosdep::install()` retained as fallback for unknown installers.
- [x] Dry-run logs exact installer commands via tracing.
- [x] Per-key failure logging with warnings (non-fatal).

### Acceptance criteria

- [x] Design doc written: `docs/design/rosdep-resolve.md`.
- [ ] Existing integration tests pass.
- [ ] `just ci` passes.

---

## 13.3 Full REP-149 conditional expression parser

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

### 13.3.1 Tokeniser and parser — DONE

**File:** `crates/rosup-core/src/condition.rs`

- [x] Token types: `LParen`, `RParen`, `And`, `Or`, `Op(CmpOp)`,
  `Var(String)`, `Literal(String)`.
- [x] Tokeniser handles: `$IDENT` variables, quoted strings (single/double),
  unquoted literals, all 6 comparison operators, `and`/`or` keywords,
  parentheses, whitespace skipping.
- [x] Recursive-descent parser producing AST: `Expr::Compare`,
  `Expr::And(Vec)`, `Expr::Or(Vec)`.
- [x] `and` binds tighter than `or` (correct precedence).
- [x] Parse errors fall back to `true` (conservative).

### 13.3.2 Evaluator — DONE

- [x] `eval_condition(condition)` — default variables: `$ROS_VERSION` = `"2"`.
- [x] `eval_condition_with_vars(condition, vars)` — custom variable bindings.
- [x] Unresolved variables → `true` (conservative).
- [x] String comparison for all operators (per REP-149 spec).

### 13.3.3 Fixtures — DONE

- [x] `condition_and_or.xml` — compound `and`/`or` conditions.
- [x] `condition_parens.xml` — parenthesised grouping.
- [x] Quoted string and malformed input covered by unit tests in
  `condition.rs`.

### 13.3.4 Tests — DONE

20 unit tests in `condition.rs`:
- [x] Tokeniser: simple comparison, quoted strings, and/or, parens,
  all operators, empty input.
- [x] Evaluator: simple ROS_VERSION, and expression, or expression,
  precedence (and > or), parenthesised grouping, unresolved variables
  (in and, in or), quoted strings, all comparison operators, malformed
  input, backwards compatibility.

2 integration tests in `package_xml.rs`:
- [x] `compound_and_or_conditions` — fixture-based.
- [x] `parenthesised_conditions` — fixture-based.

### Acceptance criteria

- [x] Existing `conditional_deps.xml` tests pass unchanged.
- [x] `just ci` passes (255 tests, 0 failures).

---

## Sequencing

These three work streams are independent and can be done in parallel. However,
a suggested priority order:

1. **13.3** (REP-149 parser) — self-contained, improves correctness now,
   no external deps.
2. **13.2** (rosdep research) — research-first, low risk, informs future
   resolver improvements.
3. **13.1** (sparse-checkout) — largest scope, biggest payoff for large
   workspaces, benefits from 3.4 fixes (bare clone + worktree rewrite).
