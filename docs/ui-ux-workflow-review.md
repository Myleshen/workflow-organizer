# devx UI, UX, and Workflow Review

## Review contract

This audit reviews the current terminal application for mixed-experience developers. It treats keyboard flows, prompts, output, errors, external application handoffs, and recovery as the UI.

Each finding is written so another agent can implement it without rediscovering the problem. Evidence labels mean:

- **Reproduced:** observed through a safe command during this review.
- **Source-confirmed:** directly follows from current control flow.
- **Risk:** requires a targeted runtime/failure test before claiming a defect.

Severity meanings:

- **P0:** credible data-loss/security blocker.
- **P1:** major correctness, safety, or core-workflow failure.
- **P2:** material usability, reliability, accessibility, or maintainability problem.
- **P3:** polish or lower-frequency debt.

## Product decisions already provided

- Target mixed-experience developers.
- Require confirmation for destructive actions.
- On reset, separately offer to remove an installed Raycast script.
- After worktree removal, separately offer to delete the local branch.
- Never bundle or imply remote branch deletion.
- Linux is desired if the effort is reasonable; current review finds it moderate and bounded.
- Keep this audit and the complete mapping in repository Markdown.

## Pending taste decision

**Recommendation:** use a minimal native terminal style. Present concise phase/progress lines, consistent status words, restrained semantic color, and actionable errors without decorative panels or icons. Keep automation output as a separate mode rather than degrading the interactive experience.

This is a recommendation, not a recorded final decision.

## Findings index

| ID | Severity | Area | Summary |
|---|---|---|---|
| F-01 | P1 | Overlay safety | Replacement can change file permissions and incomplete rollback leaves residue |
| F-02 | P1 | State safety | Configuration saves are non-atomic and concurrent invocations can lose updates |
| F-03 | P1 | Destructive workflow | Direct destructive commands violate required confirmation policy |
| F-04 | P1 | Partial success | Successful mutations can be reported as failures after launcher errors |
| F-05 | P1 | Clone | Repository URLs without `.git` are rejected |
| F-06 | P1 | Documentation | Reset promises Raycast cleanup that is not implemented |
| F-07 | P2 | Empty states | Empty pickers silently behave like cancellation |
| F-08 | P2 | Progress | Long synchronous operations appear frozen |
| F-09 | P2 | Onboarding | Setup is all-or-nothing and cannot manage existing roots |
| F-10 | P2 | Worktree source | Documented default-branch fallback is absent |
| F-11 | P2 | Worktree lifecycle | No separate local branch cleanup offer |
| F-12 | P2 | Diagnostics | Doctor can declare readiness without a usable project workflow |
| F-13 | P2 | Search | Query option injection and forced ANSI color |
| F-14 | P2 | Status accuracy | Git errors are displayed as dirty state |
| F-15 | P2 | Performance | Picker computes Git status serially for every row |
| F-16 | P2 | Overlay lifecycle | Mappings and overlays cannot be removed interactively |
| F-17 | P2 | Diff UX | Large diffs have no summary, paging, or sensitive-value policy |
| F-18 | P2 | Portability | Core is portable but setup/defaults/diagnostics are macOS-hardcoded |
| F-19 | P2 | Recovery | Mutating workflows lack transaction-aware recovery messages |
| F-20 | P2 | Testing | Core interactive/destructive workflows lack integration coverage |
| F-21 | P3 | Terminology | Success, cancellation, and unregister wording is inconsistent |
| F-22 | P3 | Ranking | Direct opens do not affect recency/frequency ranking |
| F-23 | P3 | Responsive UI | Long values and short terminals have no layout strategy |
| F-24 | P3 | Accessibility | Output and picker behavior are not tested for assistive/redirected use |
| F-25 | P3 | Validation | Names, launchers, root overlap, and control characters are under-validated |
| F-26 | P3 | Security guidance | Overlay secret handling and filesystem permissions are undefined |
| F-27 | P3 | Discovery | Recursive scans have limited exclusions and no scale contract |
| F-28 | P3 | Help/docs | Command docs contain contradictions and platform-specific remediation |

## Implementation-ready findings

### F-01: Preserve destination metadata and cleanly roll back overlay writes

**Severity:** P1  
**Evidence:** Source-confirmed in `write_config_batch()` at `src/main.rs:1642-1667`.

**Problem:** Replacement files are created with default process permissions and renamed over existing files. This can change permission bits and other metadata. If staging a later file fails, earlier temp files are not cleaned. If a rename fails mid-batch, applied files are recreated with `fs::write`, which does not restore complete metadata, and unprocessed staged files can remain.

**User impact:** Applying a valid overlay can unexpectedly change executable/read restrictions or leave `.devx-*.tmp` files. A partial failure can leave the checkout in a state the success model says should not occur.

**Required behavior:** Stage every file, preserve relevant destination metadata, and guarantee cleanup. Do not begin replacement until all staging succeeds. On failure, report exactly which destinations changed or were restored.

**Acceptance criteria:**

1. Existing Unix permission bits remain identical after a successful apply.
2. A simulated staging failure leaves every destination unchanged and no devx temp files.
3. A simulated second-rename failure restores prior content and permissions for the first destination and removes all staged files.
4. Success prints a batch summary only after every replacement completes.
5. Integration tests cover success, staging failure, mid-commit failure, and read-only destinations.

### F-02: Make configuration persistence atomic and concurrency-safe

**Severity:** P1  
**Evidence:** Source-confirmed in `save_config()` at `src/main.rs:2484-2487` and load-modify-save command patterns.

**Problem:** The complete TOML file is written directly. A crash can truncate it, and two shell/Raycast invocations can load the same version and overwrite each other's updates.

**User impact:** Registry, usage, root, or overlay state can be corrupted or silently lost.

**Required behavior:** Use an adjacent temp file plus atomic rename and an inter-process lock or optimistic revision check around read-modify-write operations. Preserve config permissions.

**Acceptance criteria:**

1. Interrupted writes cannot leave partial TOML at `config.toml`.
2. Two concurrent mutations cannot silently discard one update.
3. Lock contention has a concise wait/error message and bounded behavior.
4. Existing valid config remains recoverable if temp cleanup is needed.
5. Concurrency and interrupted-write integration tests run in CI.

### F-03: Apply the required confirmation policy to direct destructive commands

**Severity:** P1  
**Evidence:** Source-confirmed at `project remove` (`src/main.rs:929-937`) and direct worktree removal (`src/main.rs:1307-1341`).

**Problem:** Picker removal confirms, but `devx worktree remove`, `--force`, and project unregister mutate immediately. This violates the requested product policy that destructive actions always confirm.

**User impact:** A typo or stale shell history can unregister a project or delete a worktree. `--force` can discard local changes immediately.

**Required behavior:** Confirm every destructive command. For automation, define one explicit bypass such as `--yes`; do not overload `--force` as both data-loss permission and confirmation bypass. A non-TTY invocation without bypass must fail safely rather than hang.

**Acceptance criteria:**

1. Clean and dirty direct worktree removals show the target path and consequence, defaulting to No.
2. Forced dirty removal explicitly says uncommitted changes will be discarded.
3. Project removal says it unregisters only and does not delete files.
4. Non-TTY destructive calls require an explicit documented bypass.
5. Cancelled confirmations perform no Git or state mutation.

### F-04: Separate completed mutations from optional launcher handoff failures

**Severity:** P1  
**Evidence:** Source-confirmed in worktree creation (`src/main.rs:1275-1304`), project setup (`src/main.rs:1700-1730`), and global overlay creation (`src/main.rs:1733-1749`).

**Problem:** These workflows mutate Git/files/state before launching an external app. If the launcher fails, the command exits as an error before printing mutation success, making users likely to retry an already-completed operation.

**User impact:** Confusing duplicate attempts, collision errors, and uncertainty about what must be repaired.

**Required behavior:** Report the durable mutation as complete before optional app handoffs. Treat launcher failure as partial success with an exact manual recovery command/path.

**Acceptance criteria:**

1. A worktree saved successfully is reported as created even when editor or terminal launch fails.
2. Overlay mapping/file creation is reported as complete when config-editor launch fails.
3. Exit semantics distinguish total failure from completed-with-warning, with a documented choice.
4. Tests inject failures at each launcher boundary and assert truthful output/state.

### F-05: Accept standard clone URLs without `.git`

**Severity:** P1  
**Evidence:** Source-confirmed in `repository_name_from_url()` at `src/main.rs:1016-1027`.

**Problem:** `strip_suffix(".git").unwrap_or_default()` converts a valid basename without `.git` to an empty string.

**User impact:** Common HTTPS clone URLs are rejected before Git runs.

**Required behavior:** Remove `.git` when present; otherwise keep the derived basename.

**Acceptance criteria:**

1. HTTPS, SSH, SCP-style, trailing-slash, with-`.git`, and without-`.git` valid URLs derive the expected name.
2. Empty, `.`, and `..` names remain rejected.
3. Unit tests cover the complete URL matrix.

### F-06: Implement the documented, separately confirmed Raycast cleanup

**Severity:** P1  
**Evidence:** Source-confirmed discrepancy between `README.md:40-49`, `docs/devx.1`, and `reset()` at `src/main.rs:675-697`.

**Problem:** Reset claims to offer Raycast script removal but only deletes the devx config directory.

**User impact:** Users believe reset returned the machine to a clean state while an integration remains installed.

**Required behavior:** After config reset confirmation, detect the exact bundled script path and separately ask whether to remove it. Do not remove a different or user-modified file without making that fact explicit.

**Acceptance criteria:**

1. Absent script causes no extra prompt.
2. Present script receives a separate default-No confirmation.
3. Declining leaves the script and says so.
4. Accepting removes only the target script and cleans empty devx-owned directory when safe.
5. Tests cover absent, bundled, modified, declined, accepted, and permission-failure states.

### F-07: Replace silent empty pickers with actionable empty states

**Severity:** P2  
**Evidence:** Source-confirmed in `select_table()`/`select_many()` and callers; isolated `project list` also produced no output.

**Problem:** Zero projects, primary projects, or worktrees are sent to `fzf`. Exit code 1 is treated like cancellation. Users cannot tell whether there was nothing to choose, they pressed Escape, or picker execution failed to produce a selection.

**User impact:** Dead-end workflows with no recovery guidance, especially during onboarding.

**Required behavior:** Callers must preflight collections and print context-specific empty guidance before starting `fzf`. Cancellation should be consistently acknowledged only when useful.

**Acceptance criteria:**

1. Open with no projects suggests `project add-root`, `project add`, or setup.
2. Remove with no worktrees says no worktrees exist and does not launch `fzf`.
3. Worktree create with no primaries explains the requirement.
4. Multi-select with no candidates and user selecting none are distinct states.
5. Tests verify no `fzf` process starts for empty collections.

### F-08: Add concise progress for operations that can take noticeable time

**Severity:** P2  
**Evidence:** Source-confirmed synchronous scans/subprocess capture; runtime commands show no phase output.

**Problem:** Recursive discovery, status collection, clone/fetch, worktree creation, config discovery, and merge validation can remain silent long enough to look frozen.

**User impact:** Users interrupt healthy operations or repeat commands, particularly on large/network roots.

**Required behavior:** Use minimal native phase lines and preserve native Git progress where practical. Avoid decorative animation when output is redirected.

**Acceptance criteria:**

1. Interactive scans show current root and final repository count.
2. Clone/fetch/worktree creation identify the active phase and target.
3. Long config discovery/apply identifies file counts.
4. Redirected/non-TTY output remains stable and contains no spinner control sequences.
5. Cancellation and failure name the phase that stopped.

### F-09: Make setup resumable and support root lifecycle management

**Severity:** P2  
**Evidence:** Source-confirmed at `setup()` and `configure_roots()` (`src/main.rs:629-779`).

**Problem:** Launcher choices, root entry, full scanning, and save form one transaction in memory. A late error loses earlier choices. Existing roots can only be added, not renamed or removed through setup.

**User impact:** Repeated onboarding and manual TOML editing for ordinary maintenance.

**Required behavior:** Split setup into reviewable sections, persist valid choices before expensive scanning, and provide add/rename/remove root actions with confirmation for removal from devx state.

**Acceptance criteria:**

1. Setup displays a final summary before committing changed settings.
2. Scan failure preserves accepted launcher/root configuration and explains cache status.
3. Existing roots can be listed, renamed, and removed without editing TOML.
4. Removing a scan root explicitly says repositories are not deleted.
5. Ctrl+C at each stage has tested, documented persistence semantics.

### F-10: Align default-branch behavior with documentation

**Severity:** P2  
**Evidence:** Source-confirmed discrepancy at `README.md:221-224` and `fetch_default_branch()` (`src/main.rs:2361-2378`).

**Problem:** Documentation promises fallback among conventional branches, but implementation requires `origin/HEAD`.

**User impact:** Worktree creation fails on valid repositories with an unset remote HEAD.

**Required behavior:** Make an explicit product choice and align code/docs. Recommended order: resolved `origin/HEAD`, uniquely available conventional branch, otherwise ask/require an explicit base branch rather than guessing ambiguously.

**Acceptance criteria:**

1. Correct `origin/HEAD` remains authoritative.
2. Missing HEAD with exactly one supported fallback succeeds and reports that fallback.
3. Multiple plausible branches do not silently choose one.
4. Failure lists discovered remote branches and an exact recovery command/flag.
5. Real temporary-remote integration tests cover all branches.

### F-11: Offer separate local branch deletion after worktree removal

**Severity:** P2  
**Evidence:** Requested product behavior; current removal stops after deleting the checkout and state.

**Problem:** Users can accumulate stale local branches, but branch deletion has materially different consequences from worktree directory removal.

**Required behavior:** After successful worktree removal, offer a second default-No confirmation to delete the associated local branch. Never delete or offer to delete the remote branch in this workflow.

**Acceptance criteria:**

1. Prompt identifies exact local branch and states remote branches are untouched.
2. Declining leaves the branch and still reports worktree removal success.
3. Accepted deletion uses safe `git branch -d` by default.
4. Unmerged branch refusal explains a separate explicit force path; it is never silently escalated to `-D`.
5. Detached worktrees skip branch deletion with a concise explanation.

### F-12: Define and report operational readiness accurately

**Severity:** P2  
**Evidence:** Reproduced: isolated `doctor` printed `warning no scan roots` and then `devx is ready.`

**Problem:** The command distinguishes required components from warnings, but its final message implies the main project-opening workflow is usable with no projects or roots. It also validates process/application discoverability rather than an end-to-end launcher handoff.

**User impact:** False confidence immediately followed by an empty picker.

**Required behavior:** Report separate states such as ready, ready with setup remaining, and blocked. Check workflow-specific optional dependencies (`rg`, `tmux`, Raycast) only when relevant and state limitations precisely.

**Acceptance criteria:**

1. No roots and no explicit projects produces `setup incomplete`, not unconditional ready.
2. Explicit projects without roots can still be considered operational.
3. Diagnostics distinguish required, configured, optional, and workflow-disabled components.
4. Linux diagnostics never recommend macOS-only tools.
5. Invalid TOML produces recovery guidance instead of preventing all remaining checks.

### F-13: Make config search safe for option-like queries and output contexts

**Severity:** P2  
**Evidence:** Source-confirmed at `src/main.rs:1548-1557`.

**Problem:** Query is passed before file paths without a `--` delimiter and color is always forced. A query beginning with `-` can be interpreted as an `rg` option. Redirected output and `NO_COLOR` still contain ANSI sequences.

**User impact:** Incorrect errors/behavior and polluted logs or pipes.

**Required behavior:** Separate options from positional arguments and select color from TTY/`NO_COLOR` state or a user flag.

**Acceptance criteria:**

1. Queries beginning with `-` are treated as patterns.
2. Redirected output has no ANSI by default.
3. `NO_COLOR` is honored.
4. Interactive output may use restrained semantic color.
5. Tests cover no matches, invalid regex, option-like query, missing `rg`, pipe, and Unicode.

### F-14: Represent Git-status failures as unknown, not dirty

**Severity:** P2  
**Evidence:** Source-confirmed in `is_dirty()` at `src/main.rs:2219-2223`.

**Problem:** Every Git error becomes `dirty`. Dirty has a specific user meaning and may trigger destructive-force wording even when the repository is inaccessible or Git failed.

**User impact:** Misleading picker state and unsafe removal framing.

**Required behavior:** Model clean, dirty, and unknown/error separately. Removal must not convert unknown status into permission to force-delete without an explicit diagnostic.

**Acceptance criteria:**

1. Picker displays `unknown` for status failures.
2. The error detail is available without flooding the row.
3. Removal blocks or asks a distinct confirmation when status is unknown.
4. Tests cover missing path, permission error, non-repository, clean, and dirty states.

### F-15: Avoid serial Git-status work on every picker render

**Severity:** P2  
**Evidence:** Source-confirmed in `select_project()` and `project_picker_entry()`.

**Problem:** Each row invokes `git status --porcelain` sequentially before `fzf` opens. Cost grows linearly and is dominated by slow repositories.

**User impact:** Picker startup becomes slow with many or network-mounted projects and has no progress indication.

**Required behavior:** Establish a scale target, then cache status briefly, parallelize with a conservative bound, defer status, or omit it from the initial picker. Do not create unbounded subprocesses.

**Acceptance criteria:**

1. A documented project-count benchmark has an acceptable picker startup target.
2. One slow repository cannot indefinitely hide the entire picker without feedback/timeout.
3. Status freshness semantics are documented.
4. Load/performance tests include hundreds of repositories and simulated slow status calls.

### F-16: Add complete overlay mapping and cleanup workflows

**Severity:** P2  
**Evidence:** Source-confirmed command surface has add/setup/list/apply/search but no unmap/remove.

**Problem:** Once mapped, a file cannot be unmanaged or its project/global overlay cleaned through the CLI.

**User impact:** Stale mappings cause apply failures or unintended changes; users must edit TOML and filesystem state manually.

**Required behavior:** Add an interactive and direct unmap flow. Separate mapping removal from overlay-file deletion and confirm each destructive deletion.

**Acceptance criteria:**

1. User can unmap one file while preserving overlays.
2. User can separately confirm deletion of project and/or global overlay files.
3. Output explains global overlay impact across mapped projects.
4. Empty parent directories are removed only when safe.
5. Applying after unmap ignores the removed mapping.

### F-17: Make large and sensitive diff review manageable

**Severity:** P2  
**Evidence:** Source-confirmed at `src/main.rs:1602-1624`.

**Problem:** Every full diff is printed before one confirmation. There is no changed-file/line summary, pager, per-file navigation, or explicit sensitive-value policy.

**User impact:** Important changes are missed in scrollback; secrets may be exposed in terminal history or captured logs.

**Required behavior:** Print a concise batch summary, page interactive diffs safely, and define whether values are shown, redacted, or user-controlled. Preserve a plain noninteractive preview mode.

**Acceptance criteria:**

1. Preview starts with changed file and line counts.
2. Large TTY previews use a pager or deliberate navigation with a reliable exit path.
3. Redirected preview remains plain and complete.
4. Secret-display behavior is explicitly documented and tested.
5. Confirmation repeats target project and file count.

### F-18: Introduce an explicit platform abstraction for practical Linux support

**Severity:** P2  
**Evidence:** Source-confirmed macOS calls/defaults in `src/main.rs`; core workflows are platform-neutral.

**Problem:** Setup, launcher defaults/discovery/validation, diagnostics, help, Homebrew guidance, Raycast, and Ghostty workspace assume macOS. Linux users cannot get a coherent first-run experience despite a portable core.

**Required behavior:** Add OS-aware defaults and capabilities without scattering `cfg` checks through workflows. Keep Raycast and AppleScript optional macOS integrations; use generic command discovery and tmux workspace on Linux.

**Acceptance criteria:**

1. `devx init`, setup, doctor, project discovery/open, worktrees, picker, overlays, search, and tmux workspace run on a supported Linux target.
2. Linux defaults use installed/common commands or prompt for explicit commands; they never use `open -a`.
3. Raycast and AppleScript are reported unavailable rather than missing requirements.
4. Installation hints are platform-aware and do not assume Homebrew.
5. macOS and Linux CI include unit and CLI integration tests.

**Effort assessment:** Moderate. Core Git/state/overlay code remains. Most work is concentrated in launchers, application discovery, optional integrations, diagnostics, docs, and test infrastructure.

### F-19: Add recovery semantics to multi-step mutations

**Severity:** P2  
**Evidence:** Source-confirmed in clone/cache refresh, worktree Git/save, reset/script, and overlay workflows.

**Problem:** Failures after an external mutation often produce a generic error without saying what completed, what remains, or how to resume.

**User impact:** Users repeat unsafe operations or manually inspect state.

**Required behavior:** Define commit points and partial-success messages for each multi-step command. Where rollback is unsafe, prefer truthful recovery instructions over pretending atomicity.

**Acceptance criteria:**

1. Worktree Git success plus config-save failure identifies the created path/branch and exact repair action.
2. Clone success plus cache-save failure reports clone success and refresh recovery.
3. Reset config success plus Raycast cleanup failure reports both states independently.
4. Failure-injection tests cover every boundary after the first mutation.

### F-20: Add end-to-end coverage for user-critical workflows

**Severity:** P2  
**Evidence:** Existing 25 tests pass but are unit-level; no CI configuration is present.

**Problem:** The highest-risk behavior is process orchestration, prompts, Git mutation, filesystem replacement, and platform integration, but tests largely exercise helpers.

**Required behavior:** Create a CLI integration harness with isolated `HOME`/`XDG_CONFIG_HOME`, disposable Git repositories/remotes, controlled fake executables, and prompt input/output assertions.

**Acceptance criteria:**

1. CI runs format, Clippy with warnings denied, unit tests, and CLI integration tests.
2. Integration coverage includes init/setup state, empty pickers, clone URL matrix, worktree create/remove/cancel, overlay preview/apply/rollback, search, and doctor.
3. macOS-specific tests are clearly separated from portable tests.
4. Linux CI is added when Linux support begins.
5. Tests assert filesystem/Git state, not output alone.

### F-21: Normalize terminology and completion/cancellation messages

**Severity:** P3  
**Evidence:** Source-confirmed output vocabulary across commands.

**Problem:** Similar outcomes use `Created`, `Configured`, `Updated`, `Registered`, `Removed`, or silence. `project remove` says `Removed`, which can imply files were deleted. Some cancellations speak; others silently return.

**Required behavior:** Define a small vocabulary: registered/unregistered for state, created/deleted for files/Git objects, opened for handoffs, unchanged/cancelled for no mutation, warning/error for degraded/failure.

**Acceptance criteria:**

1. Project removal says `Unregistered` and explicitly preserves the checkout.
2. Cancellation behavior is consistent by interaction type.
3. Successful batch operations end with one concise summary.
4. Snapshot tests cover human-facing output contracts.

### F-22: Decide whether direct opens contribute to ranking

**Severity:** P3  
**Evidence:** Source-confirmed: `record_open()` is called only by `pick_open()`.

**Problem:** Ranking describes open frequency/recency but excludes `devx open` and `devx workspace`, so active projects may rank as unused.

**Required decision:** Either count every successful project open or document that ranking reflects picker selections only. Recommendation: count all successful devx open/workspace paths through one shared service.

**Acceptance criteria:**

1. Product semantics are documented.
2. Usage is recorded only after the intended launch commit point.
3. Failed launches do not inflate ranking unless partial-success policy explicitly says otherwise.
4. Stale usage entries are pruned when no project matches.

### F-23: Define narrow, long-value, and short-terminal layouts

**Severity:** P3  
**Evidence:** Source-confirmed single 100-column breakpoint and percentage heights.

**Problem:** Long names/branches/paths can overflow or dominate rows. Very short terminals can make 40%/70% pickers impractical. There is no truncation or detail-view convention.

**Required behavior:** Establish column priorities and deterministic truncation, with full selected-item details available before destructive confirmation.

**Acceptance criteria:**

1. Test widths 40, 80, 99, 100, 101, and 200 and short heights.
2. Name remains distinguishable; less critical columns truncate first.
3. Destructive confirmation shows untruncated name/path/branch.
4. Unicode width does not corrupt alignment.

### F-24: Establish accessibility and redirected-output behavior

**Severity:** P3  
**Evidence:** No accessibility tests; forced color and tabular `fzf` UI are source-confirmed.

**Problem:** Keyboard-first is positive, but screen-reader behavior, focus transitions, color preferences, tabular interpretation, and external app launches are undefined.

**Required behavior:** Document keyboard/cancel behavior, honor terminal capability and `NO_COLOR`, and provide direct-command equivalents for interactive workflows where feasible.

**Acceptance criteria:**

1. Every core picker action has a documented direct command.
2. Status meaning never depends only on color.
3. Screen-reader/manual keyboard testing covers setup, open, removal, and apply.
4. Redirected outputs contain no interactive control sequences.

### F-25: Tighten validation before persistence or process launch

**Severity:** P3  
**Evidence:** Source-confirmed gaps in project/root naming, launcher editing, root overlap, and table-delimited values.

**Problem:** Empty/whitespace aliases, tabs/newlines/control characters, overlapping scan roots, duplicate root paths in direct commands, and launcher placeholder validity are incompletely checked. Workspace command input uses whitespace splitting and cannot represent quoted arguments reliably.

**Required behavior:** Validate names for terminal/table safety, normalize aliases, detect overlapping roots with a deliberate policy, and provide a structured custom-launcher editor or explicit TOML guidance with validation.

**Acceptance criteria:**

1. Empty and control-character names are rejected with actionable messages.
2. Duplicate and nested roots receive explicit warnings/decisions.
3. Required launcher placeholders are validated for each role.
4. Workspace commands can represent arguments containing spaces without ambiguous splitting.
5. Validation occurs before expensive scans or state mutation.

### F-26: Define overlay secret storage and display policy

**Severity:** P3  
**Evidence:** Risk based on local config-overlay purpose and current plain files/diffs.

**Problem:** Overlays may contain credentials, but directory/file modes, backup expectations, source-control warnings, and diff/search exposure are not defined.

**Required behavior:** Decide whether secrets are supported. If supported, create private config/overlay paths, preserve secure modes, warn about terminal diff/search exposure, and document backup/synchronization risks.

**Acceptance criteria:**

1. Product documentation explicitly states whether secrets belong in overlays.
2. Newly created sensitive state uses restrictive platform-appropriate permissions.
3. Doctor can warn about overly broad permissions without printing secret content.
4. Search/diff behavior is consistent with the chosen exposure policy.

### F-27: Bound recursive discovery behavior

**Severity:** P3  
**Evidence:** Source-confirmed recursive `read_dir` traversal at `src/main.rs:1176-1227` and config discovery at `src/main.rs:1761-1789`.

**Problem:** Repository scan has no ignore list, maximum depth, progress, or scale contract. Config discovery has a small fixed ignore list. Permission errors abort the complete scan.

**User impact:** Large roots, generated directories, network mounts, or one inaccessible subtree can make setup slow or fail entirely.

**Required behavior:** Define traversal policy, configurable exclusions, permission-error handling, and progress. Avoid following symlink cycles explicitly even if current platform behavior often prevents them.

**Acceptance criteria:**

1. Scan reports skipped inaccessible directories and continues or fails according to documented strictness.
2. Users can exclude heavy directories.
3. Symlink behavior is explicit and tested.
4. Benchmarks cover realistic large roots.

### F-28: Reconcile documentation, help, and platform guidance

**Severity:** P3  
**Evidence:** Source-confirmed contradictions listed in `docs/project-workflow-map.md`.

**Problem:** README, man page, and implementation disagree on reset cleanup, branch fallback, search status, cache refresh semantics, and destructive direct usage. Help text identifies the complete product as macOS even though Linux is desired.

**Required behavior:** Establish one behavior source and test generated/static docs against the command surface and supported platform matrix.

**Acceptance criteria:**

1. Reset, branch resolution, config search, worktree cache, and confirmation docs match implementation.
2. Installation/remediation is platform-aware.
3. README, man page, and `--help` command lists are checked in CI.
4. Unsupported platform integrations are labeled optional rather than silently presented as universal.

## Recommended delivery order

### Safety and correctness

Implement F-01 through F-06 first. These address data integrity, destructive confirmation, misleading partial failure, a broken common clone input, and a false cleanup promise.

### Core workflow clarity

Implement F-07 through F-14 next. These fix dead ends, progress, onboarding resilience, branch lifecycle, diagnostics, search behavior, and status accuracy.

### Scale and lifecycle completeness

Implement F-15 through F-20 next. These improve performance, complete overlay management, improve diff review, establish Linux support boundaries, add recovery, and create integration coverage.

### Consistency and long-term quality

Implement F-21 through F-28 after product decisions are confirmed.

## Suggested issue template for another agent

Use one finding per implementation task:

```text
Implement <finding ID and title> from docs/ui-ux-workflow-review.md.

Constraints:
- Preserve unrelated existing worktree changes.
- Follow the stated required behavior exactly.
- Satisfy every acceptance criterion or report the specific blocker.
- Add unit and integration tests at the correct boundary.
- Keep human output concise and use the minimal native style unless the product decision changes.
- Do not introduce macOS assumptions into portable core code.
- Run cargo fmt -- --check, cargo clippy --all-targets --all-features -- -D warnings, and cargo test.
```

## Verification completed for this review

- `cargo test`: 25 passed, 0 failed.
- `cargo clippy --all-targets --all-features -- -D warnings`: passed.
- `cargo fmt -- --check`: passed.
- Help surfaces for root, project, config, workspace, and worktree removal were inspected.
- `init`, `doctor`, and empty `project list` were run against isolated state.
- Source control status and existing diffs were inspected before adding these documents.

## Residual review limits

The review did not launch or mutate the user's real applications, repositories, worktrees, Raycast setup, or overlays. Full interactive `fzf` behavior, AppleScript/Ghostty integration, failure injection, screen-reader behavior, large-data performance, and Linux execution remain runtime validation work. Findings clearly marked as risks must be reproduced in a controlled harness before being called confirmed defects.
