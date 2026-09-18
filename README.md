# devx

`devx` is a macOS command-line helper for opening registered Git projects and
worktrees, and safely applying layered local configuration overlays.

## Install and initialize

```sh
cargo install --path .
devx init
```

By default, configuration is written to `~/.config/devx`. Set
`XDG_CONFIG_HOME` to use a different base directory.

For a complete first-time setup, use the interactive onboarding command instead
of configuring files by hand:

```sh
devx setup
```

It discovers applications from `/Applications`, `~/Applications`, and Homebrew
Cask installations; lets you choose the IDE, terminal, lightweight config
editor, and Raycast terminal; prompts for one or more scan roots; refreshes the
project cache; and can install the Raycast command. Selections are saved in
`~/.config/devx/config.toml` for all later commands.

`setup` also prints optional Homebrew recommendations for the opinionated stack:
Ghostty, Zed, and LazyGit. It does not install or require them. LazyGit becomes
required only while the default workspace workflow is enabled.

Change only application choices later:

```sh
devx launcher edit
devx launcher list
```

Edit all interactive settings using their current values as the starting point:

```sh
devx edit
```

This walks through the configured applications, named launcher profiles,
workspace behavior, and additional scan roots. `devx pick` asks which opening
profile to use after you choose a project.

To test another workflow from a clean slate, remove all devx configuration and
overlays with a confirmation prompt. This never changes repositories or Git
worktrees:

```sh
devx reset
```

If the optional Raycast Script Command is installed, reset separately offers to
remove it. `devx reset --yes` confirms both actions for non-interactive use.
Symlinked Raycast path components are refused rather than followed, and an
existing script is revalidated after confirmation before replacement or removal.

The installed binary includes this complete manual, so users do not need the
repository to learn the workflows:

```sh
devx man
devx --help
devx project --help
```

Verify a binary-only setup after installation:

```sh
devx doctor
```

It checks Git, `fzf`, configured applications, the local configuration, and the
Raycast Script Command, with commands to resolve missing dependencies.

Generate shell completions without the repository:

```sh
mkdir -p ~/.zfunc
devx completions zsh > ~/.zfunc/_devx
echo 'fpath=(~/.zfunc $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc
```

For fish:

```sh
mkdir -p ~/.config/fish/completions
devx completions fish > ~/.config/fish/completions/devx.fish
```

Interactive selection requires [fzf](https://github.com/junegunn/fzf):

```sh
brew install fzf
```

All interactive `fzf` prompts match case-insensitively. Press `Esc` to cancel a
picker, or `Ctrl+C` at any prompt to immediately stop `devx` and its active
child process. `devx` does not continue to later writes after cancellation.

## Projects and worktrees

Register scan roots once, then refresh the cached repository list whenever
repositories or externally-created worktrees change. Clones made by `devx`
refresh the cache automatically. Worktree creation registers the new worktree
directly without rescanning the cache.

```sh
devx project add-root ~/dev
devx project add-root ~/learning
devx project refresh
devx project list
devx open my-service
```

When two discovered repositories have the same final directory name, both get
their scan-root prefix. For example, `~/dev/payments` and
`~/learning/payments` become `dev-payments` and `learning-payments`.

Clone a repository into a named root from any directory:

```sh
devx project clone dev git@github.com:org/my-service.git
```

This creates `~/dev/my-service` and does not open applications afterward.
Use `devx pick` and choose **Clone repository** to select the scan root
interactively, enter the Git URL, and open the new checkout in the configured
editor and default workspace when cloning completes. Use `project add` only for
a repository outside a scan root, or to assign an explicit alias.

For a worktree registered manually rather than created by `devx`, map it to the
primary project's overlays. The primary may be a scan-root-discovered project;
use its `project list` name as the shared overlay owner:

```sh
devx project set-template my-service-feature my-service
```

`devx open` uses the `editor-terminal` profile by default. Choose an explicit
profile for deterministic agent-friendly behavior:

```sh
devx open my-service --profile intellij
devx open my-service --profile zed
devx open my-service --profile terminal
devx open my-service --profile workspace
```

## Workspace Default

`devx pick` asks which named profile should open the selected checkout. The
built-in workspace profile uses the configured VCS tool; the default is LazyGit.
With
Ghostty, `devx` uses Ghostty's native macOS split: a shell on the left and
LazyGit on the right. Other terminals use a tmux split only when `tmux` is
installed and their launcher includes `{command}`; otherwise they open normally.

Use the `workspace` profile when you want the VCS workspace explicitly:

```sh
devx workspace my-service
```

Disable the default picker workspace or select another VCS tool in
the interactive configuration flow:

```sh
devx workspace --configure
```

It asks whether `pick` should use workspaces, accepts a VCS command, and shows
the selected terminal behavior. Advanced users can edit
`~/.config/devx/config.toml` directly:

```toml
[workspace]
enabled = true
vcs = ["lazygit"]
```

Set `enabled = false` to make `pick` open normal terminals. `vcs` is a token
array, so alternatives such as `git gui` can be configured as
`vcs = ["git", "gui"]`.

Use `devx pick` to choose an action with `fzf`: open a registered checkout,
clone a repository, create a worktree, set up configuration overlays, or apply
them. The direct
commands remain available for scripting. The open picker lists the most
recently/frequently opened projects first, then lets `fzf` fuzzy-filter them.
Choose a project first, then choose its primary checkout or one of its
worktrees. Pickers use compact tabular rows with headers. They show name,
branch, clean/dirty state, and checkout type in narrow terminals; at 100 or more
columns they add the home-relative path. `fzf` filters the visible columns, not
the hidden selection identifier.

## Launchers

`devx init` creates `~/.config/devx/config.toml`. Derived project and Git status
metadata is stored separately in `~/.config/devx/cache.toml`; rebuild it with
`devx project refresh`. Launchers are token arrays;
`{path}` is replaced with the registered directory. The defaults open IntelliJ
IDEA and Ghostty with `open`:

```toml
[launchers]
editor = ["open", "-a", "IntelliJ IDEA", "{path}"]
terminal = ["open", "-a", "Ghostty", "{path}"]
config_editor = ["open", "-a", "Zed", "{path}"]
```

Named profiles have stable IDs for scripts and agents and friendly names for
the interactive picker:

```toml
[[launchers.profiles]]
id = "intellij"
name = "IntelliJ IDEA"
command = ["open", "-a", "IntelliJ IDEA", "{path}"]

[[launchers.profiles]]
id = "zed"
name = "Zed"
command = ["open", "-a", "Zed", "{path}"]
```

Override the terminal command for another app when you are ready. For Terminal:

```toml
terminal = ["open", "-a", "Terminal", "{path}"]
```

The command must be a token array rather than a shell string. This avoids shell
escaping problems and allows paths containing spaces to be passed safely.

## Worktree layout

Keep primary clones separate from worktrees to avoid recursive searches and IDE
indexing from crossing into sibling worktrees:

```text
~/dev/
  my-service/                         # primary checkout
  .worktrees/
    my-service/
      feature-login/                  # Git worktree
```

Create a new worktree with its branch name:

```sh
devx worktree create my-service feature/login
```

The command fetches `origin`, resolves its configured remote default branch,
creates the new branch from the current `origin/<default-branch>`, then
registers the worktree. Its default registry name is `my-service-feature-login`;
override it with `--name` when needed.

After creation, `devx` opens the new worktree in the configured editor and
terminal automatically.

Remove a worktree through the picker with `devx pick` and **Remove worktree**.
Direct and picker removal both require confirmation. `--force` permits dirty
removal but does not bypass confirmation. Non-TTY automation must pass `--yes`:

```sh
devx worktree remove my-service-feature-login
devx worktree remove my-service-feature-login --force
devx worktree remove my-service-feature-login --force --yes
```

After successful interactive removal, `devx` separately offers to delete the
exact local branch with safe `git branch -d`. Non-TTY removal retains the local
branch. Remote branches are never changed or offered for deletion.

The new entry gets `template_project = "my-service"` automatically. It shares
the main project's managed overlays while retaining its own registered name.

Existing worktrees remain where they are. `devx` discovers them through
`git worktree list`, including repository-local `.worktrees` layouts, without
moving or modifying them.

## Managed Configuration Overlays

`devx` keeps local configuration outside repositories. Set up a project once;
with no project name, `fzf` first selects a checkout and then one or more
supported configuration files found recursively in the primary repository:

```sh
devx project setup my-service
devx project setup
```

The picker searches `.yml`, `.yaml`, `.properties`, and `.env` files and supports
selecting multiple files with `TAB`. For every selected project file, setup
creates empty overlays in both layers
and opens the project overlay directory in Zed by default:

```text
~/.config/devx/configs/
  global/
    src/main/resources/bootstrap.yml
  my-service/
    src/main/resources/bootstrap.yml
```

The project overlay is stored under the primary project name, so all of its
worktrees share it. Change `config_editor` in `config.toml` to use another
lightweight editor; it uses the same safe token-array format as the other
launchers.

Create a standalone global overlay file when needed. The command prompts for a
path relative to a project root and accepts only the supported file types:

```sh
devx config global-add
# Example input: src/main/resources/bootstrap.yml
```

A global overlay is discovered by its relative path and applies to every
selected project that has a matching base file. Project-specific overlays are
still managed through `devx project setup` and are applied after the global
overlay when both exist.

Apply every mapped file to a checkout or worktree in one previewed batch:

```sh
devx config list my-service
devx config apply my-service
```

Search the mapped base configuration and matching global/project overlays with
case-insensitive ripgrep output:

```sh
devx config search my-service config-server
```

This requires `rg` (`brew install ripgrep`) only for search.

`config apply` merges `base project file < global overlay < project overlay`,
prints every unified diff, and performs no writes unless a single confirmation
is accepted. Relative paths cannot escape their configured roots.

When applying configuration from `devx pick`, global files outside the `src`
directory are offered separately for copying. Select files such as
`gradle.properties` when prompted. They are copied into the selected checkout
only when the destination does not already exist; existing files are preserved.
Files under `src` remain overlay-managed and are not offered in this copy step.

YAML overlays must use nested mappings, not dotted keys. Mappings merge
recursively, scalars replace scalars, and lists replace lists. A mapping/list/
scalar type conflict, duplicate YAML key, or dotted key that overlaps a nested
path stops the entire batch before any file is written. YAML comments and
formatting are normalized after a successful semantic merge.

`.properties` and `.env` overlays merge by key. Later layers replace earlier
values, new keys are appended, and unchanged base lines and comments are kept.
Duplicate keys in the base or either overlay are errors. `.properties` supports
one logical `key=value` or `key:value` entry per line, including backslash
continuations; escaped separators and escaped keys are not supported.

Overlay replacement revalidates destination content, file identity, and path
components after confirmation and before each replacement. It preserves Unix
permission bits, but not ownership, ACLs, or extended attributes. Staged files
and containing directories are synchronized around atomic renames. A narrow
platform-filesystem race remains between final validation and rename because
the portable Rust filesystem API does not expose directory-relative no-follow
replacement.

`rg` is required only for `devx config search`; project and file selection use
the explicit registry and `fzf`.

## Raycast Shortcut

Install the bundled Raycast Script Command directly from the binary:

```sh
devx raycast install
```

This writes `devx-pick.sh` under
`~/Library/Application Support/Raycast/Script Commands/devx`. Add that
directory in Raycast Settings, Extensions, Script Commands, Add Directory.
Search for **Devx Pick**, then assign a Raycast hotkey. The script opens the
cached `devx pick` flow using `[launchers].raycast_terminal` and runs the
binary at `~/scripts/devx`. Its default is Ghostty; change that launcher to use
another terminal.
