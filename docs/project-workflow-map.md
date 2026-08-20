# devx Product and Workflow Map

## Document purpose

This document maps the product as reviewed on 2026-08-20. It is intended to let another engineer understand the product surface, architecture, state, workflows, integrations, and test gaps before changing behavior.

This is a terminal application, not a browser or native graphical application. Its UI consists of Clap command routes and help, `fzf` pickers, `dialoguer` inputs and confirmations, terminal output and diffs, and handoffs to external applications.

## Review baseline

- Repository: `devx`
- Package: one Rust binary crate
- Version: `0.1.0`
- Rust edition: 2024
- Primary sources: `src/main.rs`, `src/state.rs`
- Product documentation: `README.md`, `docs/devx.1`
- Optional integration: `raycast/devx-pick.sh`
- Current stated platform: macOS
- Desired future platform: practical Linux compatibility where reasonable
- Primary audience: developers with mixed terminal and Git experience
- Safety preference: every destructive action should require explicit confirmation
- Worktree branch cleanup preference: offer a separate confirmation for local branch deletion; never imply or perform remote branch deletion
- Reset preference: separately confirm removal of an installed Raycast script
- Presentation under consideration: minimal native terminal output is recommended, but not yet recorded as a final product decision

The worktree was already dirty when this review began. `README.md`, `docs/devx.1`, and `src/main.rs` were modified, and `src/state.rs` was untracked. These files were reviewed as the current working product and were not changed by this review.

## Product purpose

`devx` helps a local developer:

1. Discover or explicitly register Git repositories and worktrees.
2. Find and open a project in configured editor and terminal applications.
3. Create and remove Git worktrees.
4. Open an editor plus VCS-oriented terminal workspace.
5. Maintain local global and project-specific configuration overlays outside repositories.
6. Preview and apply merged overlays back to a checkout.
7. Launch the picker through an optional Raycast Script Command.

There is no server, browser frontend, account system, authorization layer, database, HTTP API, or shared multi-user state.

## Architecture

```text
Shell / Raycast
      |
      v
Clap command parser (src/main.rs)
      |
      +-- interactive UI: fzf + dialoguer
      +-- persisted state: TOML via src/state.rs
      +-- Git workflows: git subprocesses
      +-- overlays: YAML/properties/env merge engine
      +-- launchers: configured process token arrays
      +-- workspace: Ghostty/AppleScript or terminal/tmux
      +-- diagnostics: local command and application checks
```

The application is synchronous. Commands load the complete TOML configuration, perform local filesystem/process work, and often rewrite the complete configuration.

## Technology and dependencies

| Dependency | Responsibility |
|---|---|
| `clap` / `clap_complete` | Commands, help, Zsh and Fish completions |
| `dialoguer` | Text inputs and yes/no confirmations |
| `serde` / `toml` | Persistent configuration serialization |
| `serde_yaml` | YAML parsing, validation, merge, and rendering |
| `similar` | Unified overlay diffs |
| `anyhow` | Error propagation and context |
| `tempfile` | Unit-test isolation |

External executables include `git`, `fzf`, `rg`, `open`, `brew`, `man`, `tput`, `osascript`, `tmux`, configured editors/terminals, and a configured VCS tool such as LazyGit.

## Persistent data model

`Config` in `src/state.rs` stores:

| Field | Meaning |
|---|---|
| `projects` | Explicitly registered repositories and worktrees |
| `roots` | Directories recursively scanned for repositories |
| `usage` | Picker open count and last-opened timestamp by project name |
| `launchers` | Editor, terminal, config editor, and Raycast terminal token arrays |
| `workspace` | Whether picker opens use a workspace and which VCS command runs |
| `managed_files` | Relative configuration paths associated with an overlay owner |
| `cached_projects` | Repositories/worktrees found during the latest scan |
| `cache_initialized` | Whether lazy migration/initial cache refresh has occurred |

`Paths::from_environment()` places state under `$XDG_CONFIG_HOME/devx`, or `~/.config/devx` when `XDG_CONFIG_HOME` is absent.

```text
~/.config/devx/
  config.toml
  configs/
    global/
      <relative managed path>
    <primary-project>/
      <relative managed path>
```

## Complete command surface

| Command | Main behavior |
|---|---|
| `devx init` | Create default configuration if absent |
| `devx setup` | Configure applications and scan roots, refresh cache, optionally install Raycast |
| `devx reset` | Confirm and remove devx configuration directory |
| `devx man` | Write and display embedded man page |
| `devx doctor` | Check commands, launchers, config, roots, and Raycast script |
| `devx completions zsh\|fish` | Generate completion script |
| `devx raycast install` | Install or replace bundled Raycast script |
| `devx raycast pick` | Open `devx pick` in configured Raycast terminal |
| `devx launcher edit` | Interactively select macOS applications for launcher roles |
| `devx launcher list` | Print launcher token arrays |
| `devx project add PATH [--name]` | Explicitly register a checkout |
| `devx project list` | Print available checkouts |
| `devx project remove NAME` | Remove an explicit registration |
| `devx project add-root PATH [--name]` | Add scan root and refresh cache |
| `devx project refresh` | Rescan all roots |
| `devx project clone ROOT URL` | Clone beneath a named root and refresh cache |
| `devx project setup [PROJECT]` | Select and map configuration files |
| `devx project set-template NAME OWNER` | Associate registered checkout with another overlay owner |
| `devx open NAME [flags]` | Open configured editor and/or terminal |
| `devx workspace [NAME] [--configure]` | Configure or open terminal workspace |
| `devx pick` | Select common actions through `fzf` |
| `devx worktree create PROJECT BRANCH [--name]` | Fetch, create, register, and open worktree |
| `devx worktree remove PROJECT [--force]` | Remove Git worktree and registry/cache entry |
| `devx config apply PROJECT` | Preview, confirm, and apply all overlay changes |
| `devx config list PROJECT` | Print mapped configuration paths |
| `devx config global-add` | Create a standalone global overlay |
| `devx config search PROJECT QUERY` | Search base and overlay files with `rg` |

## Interactive UI primitives

| Primitive | Implementation | Behavior |
|---|---|---|
| Single picker | `select_one()` | `fzf`, 40% height, case-insensitive |
| Table picker | `select_table()` | `fzf`, 70% height, hidden identifier column, header |
| Multi picker | `select_many()` | `fzf --multi`, 40% height |
| Project picker | `select_project()` | Branch, state, type, and optional path columns |
| Input | `dialoguer::Input` | Scan roots, names, branches, VCS command, overlay path |
| Confirmation | `dialoguer::Confirm` | Used for reset, picker worktree removal, apply, and Raycast replacement |
| Diff preview | `similar::TextDiff` | Unified diff printed directly to terminal |

Project rows switch at 100 columns. Narrow rows show name, branch, clean/dirty state, and checkout type. Wide rows add a home-relative path. There is no explicit truncation or short-terminal strategy.

## User and permission model

The only evidenced persona is a local developer using Git, a terminal, an editor, and optionally Raycast and LazyGit. Permissions are operating-system filesystem and process permissions. There are no product roles.

Configured launcher arrays can execute local programs. This is expected local configuration behavior, but it means configuration-file integrity is a security boundary.

## Workflow map

### First-time initialization

`devx init` creates default state. It does not verify dependencies or guide launcher/root setup beyond its success message.

### Interactive onboarding

`setup()` performs this sequence:

1. Load existing configuration or construct defaults.
2. Discover applications from `/Applications`, `~/Applications`, and Homebrew Casks.
3. Select editor, terminal, configuration editor, and Raycast terminal.
4. Repeatedly ask for scan roots and aliases.
5. Recursively scan roots and expand Git worktrees.
6. Save the complete configuration.
7. Optionally install Raycast integration.
8. Print optional Ghostty, Zed, and LazyGit recommendations.
9. Recommend `devx doctor`.

Selections are not saved incrementally. A late scan failure can discard earlier answers. Setup can add roots but cannot remove or edit existing roots.

### Discovery and caching

`discover_projects_in()` recursively walks every directory under each scan root until it finds a directory containing `.git`. It then stops descending into that checkout. `expand_worktrees()` invokes `git worktree list --porcelain`, gets branch information, and resolves naming collisions.

Explicit projects and cached projects are combined on use. Duplicate names are rejected. `load_config()` may refresh and save the cache as a side effect when `cache_initialized` is false.

### Open picker

`pick_open()`:

1. Combines explicit and cached projects.
2. Groups checkouts by overlay owner/primary project.
3. Ranks groups by latest open time, total opens, and then name.
4. Prompts for a project group.
5. Prompts for primary checkout or worktree.
6. Opens editor and terminal/workspace.
7. Records usage after launch calls return successfully.

Direct `devx open` does not record usage. Application spawning is treated as launch success without waiting for the application to initialize.

### Worktree creation

`worktree_create()`:

1. Resolve a primary project.
2. Reject creation from a worktree.
3. Validate the new branch with Git.
4. Derive registry name and sibling `.worktrees/<primary>/<branch>` destination.
5. Check name and path collisions.
6. Fetch and prune `origin`.
7. Resolve `refs/remotes/origin/HEAD`.
8. Run `git worktree add -b` from the remote default branch.
9. Register and save the worktree.
10. Open editor and terminal.
11. Print completion.

There is no pre-mutation summary or confirmation. If Git succeeds but state saving or launching fails, the user receives an error after some or all mutation has completed.

### Worktree removal

The picker selects only worktrees, calculates dirty state, and confirms removal. Dirty state changes the prompt to force removal and discard changes.

The direct command does not confirm. `--force` bypasses the dirty-state refusal. Removal executes Git first, removes matching state entries, saves configuration, and prints completion. It does not delete a branch.

The intended future behavior is a separate local-branch deletion offer after successful worktree removal. Remote branch deletion is out of scope and must not be implied.

### Workspace opening

Picker opens use the workspace when enabled. Direct `open` uses normal terminal launch unless `--workspace` is supplied. Direct `workspace NAME` opens the workspace.

Ghostty is detected by parsing an `open -a Ghostty` launcher and uses AppleScript for a native split. Other launchers can use tmux only when the terminal command contains `{command}`. Missing workspace dependencies fall back to a normal terminal with a warning.

### Overlay setup

`project_setup()`:

1. Resolve or interactively select a project.
2. Recursively find `.yml`, `.yaml`, `.properties`, and `.env` files while skipping selected build/tool directories.
3. Multi-select files.
4. Associate selected paths with the primary overlay owner.
5. Create empty global and project overlay files.
6. Save state.
7. Open the project overlay directory.
8. Print completion.

There is no interactive unmap or overlay deletion workflow.

### Overlay application

`config_apply()`:

1. Resolve the checkout and all mappings for its owner.
2. Canonicalize each base file and reject symlink escape from the checkout.
3. Read base, global overlay, and project overlay.
4. Merge all files in memory and validate the full batch.
5. Collect changed files and unified diffs.
6. Print every diff.
7. Confirm the complete batch once.
8. Stage replacement files beside destinations.
9. Rename staged files over destinations.
10. Attempt content rollback if a later rename fails.

YAML merges mappings recursively, replaces same-kind scalar/list values, and rejects type conflicts, dotted overlay keys, selected dotted-base overlaps, and duplicate keys detected by a custom line scanner. YAML rendering normalizes comments and formatting. Properties and env files preserve untouched lines but intentionally support only a restricted syntax.

### Configuration search

`config_search()` builds a list containing mapped base files and existing global/project overlays, then runs case-insensitive `rg` with filename, line number, and forced color. Exit code 1 is an empty result; other failures become command errors.

### Reset

`reset()` confirms and removes the complete devx configuration directory. Current implementation does not inspect or remove the separately installed Raycast script, despite current README/man-page claims.

### Diagnostics

`doctor()` checks Git, `fzf`, configured launcher commands/applications, configured workspace VCS, configuration existence, optional Raycast script, and whether scan roots are empty. It reports ready when required/configured checks pass even if no scan roots exist.

## State and feedback inventory

### Loading and progress

There is no explicit progress feedback for application discovery, repository scanning, per-project Git status, clone/fetch, worktree creation, config-file discovery, merge validation, or search. Subprocess output is generally captured, so native Git progress is not visible.

### Empty states

Explicit empty states exist for absent config during reset, no overlay mappings in list, no overlay changes, no search matches, and no supported configuration files. Project/worktree picker emptiness is passed to `fzf`, where it is indistinguishable from cancellation.

### Error states

Errors use `anyhow` context and top-level `Error:` output. There are no retry loops, resumable onboarding, transaction recovery guidance, or partial-success status models.

### Success states

Success wording includes `Created`, `Configured`, `Updated`, `Registered`, `Removed`, and silent external launch success. Overlay apply prints one `Updated` block per file but no batch summary.

### Destructive states

Interactive reset, interactive worktree removal, overlay apply, and Raycast replacement default to No. Direct project unregister and direct worktree removal are not confirmed. Worktree creation and clone mutate without a preview/confirmation.

## Platform coupling and Linux feasibility

### Portable core

These areas are already fundamentally portable across macOS and Linux:

- Rust CLI parsing and state model
- XDG configuration path handling
- Git discovery, clone, status, and worktree commands
- `fzf`, `dialoguer`, `rg`, `tput`, `man`, and tmux workflows
- Overlay parsing, validation, merge, and writes
- Generic token-array launcher execution

### macOS-specific surface

- Product description says macOS.
- Default launchers use `open -a`.
- Application discovery scans `.app` bundles and Homebrew Casks.
- Launcher validation uses `open -Ra`.
- Setup recommends Homebrew commands.
- Ghostty native splits use `osascript`.
- Raycast is macOS-only and uses `~/Library/Application Support`.
- Default Raycast shell is `zsh`.
- Several remediation messages say `brew install`.

### Estimated effort

Linux support is a moderate, bounded change rather than an architectural rewrite. A practical implementation would add OS-aware defaults and discovery, use `xdg-open` or direct configured commands on Linux, treat Raycast and AppleScript as unavailable optional features, retain tmux as the workspace implementation, and add Linux CI/integration coverage. The overlay and Git cores do not need redesign.

## Documentation inconsistencies

1. README and man page say reset offers Raycast removal; implementation does not.
2. README says worktree creation falls back among `main`, `master`, `develop`, or configured remote default; implementation only accepts `origin/HEAD`.
3. README documents implemented config search, then later says `rg` is reserved for a future search command.
4. Documentation says devx-created worktrees refresh cache automatically; creation explicitly registers the worktree but does not refresh cached discovery.
5. README calls direct worktree removal a non-interactive equivalent, while the preferred product safety policy now requires confirmation for destructive actions.

## Test strategy and observed checks

Twenty-five unit tests currently cover state serialization, path safety, naming, collision behavior, selected YAML/properties logic, overlay creation, cached projects, app discovery helpers, picker row formatting, dirty fallback, search path construction, and workspace defaults/quoting.

Checks run during review:

- `cargo test`: 25 passed
- `cargo clippy --all-targets --all-features -- -D warnings`: passed
- `cargo fmt -- --check`: passed
- `cargo run -- --help`: passed
- Isolated `devx init`: passed
- Isolated `devx doctor`: completed and exposed readiness semantics described in the audit
- Empty isolated `devx project list`: produced no output

Missing coverage includes complete CLI scenarios, real Git repositories/worktrees, interactive picker behavior, prompt cancellation, Raycast lifecycle, launcher handoffs, clone URL variants, concurrency, partial failure/rollback, permissions, accessibility, terminal-width matrices, large project collections, Linux, and documentation consistency.

## Unknowns requiring explicit product decisions

1. Confirm minimal native terminal presentation as the default style.
2. Define whether confirmations are required only on TTYs and what explicit noninteractive bypass flag automation should use.
3. Define supported Linux distributions, desktops, terminals, and installation method.
4. Define whether config output must have a stable scripting contract or JSON mode.
5. Define expected project/root scale and acceptable scan/picker latency.
6. Define whether overlays may contain secrets and required storage permissions.
7. Define expected concurrent use from shell, Raycast, or multiple terminals.
8. Define whether direct `devx open` should affect ranking.
9. Define whether Git-status failures appear as `unknown` or as a blocking error instead of `dirty`.
10. Define supported repository edge cases: bare, detached, inaccessible, submodule, network-mounted, and linked worktree-only roots.
11. Define supported Rust and macOS versions.
12. Define whether Ghostty AppleScript compatibility is a supported contract or best-effort integration.

## Runtime scenarios still required

- Onboarding with no applications, duplicate/nested roots, inaccessible paths, and cancellation at each prompt.
- Empty, single, large, duplicate-name, detached, inaccessible, and network-mounted project sets.
- Terminal widths around 40, 80, 99, 100, 101, and 200 columns and short heights.
- Worktree creation with offline origin, missing origin, missing `origin/HEAD`, collisions, and each partial failure boundary.
- Interactive and direct removal for clean, dirty, locked, detached, and externally-created worktrees.
- Overlay apply with read-only files, symlinks, permission modes, comments, large diffs, stage/rename failures, and concurrent commands.
- Search with missing `rg`, a query beginning with `-`, redirected output, `NO_COLOR`, invalid regex, and Unicode.
- Linux setup, discovery, launcher, picker, tmux workspace, and documentation checks.
