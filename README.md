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

Change only application choices later:

```sh
devx launcher edit
devx launcher list
```

To test another workflow from a clean slate, remove all devx configuration and
overlays with a confirmation prompt. This never changes repositories or Git
worktrees:

```sh
devx reset
```

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
repositories or externally-created worktrees change. Clones and worktrees made
by `devx` refresh the cache automatically.

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
Use `project add` only for a repository outside a scan root, or to assign an
explicit alias.

For a worktree registered manually rather than created by `devx`, map it to the
primary project's overlays. The primary may be a scan-root-discovered project;
use its `project list` name as the shared overlay owner:

```sh
devx project set-template my-service-feature my-service
```

`devx open` starts both configured launchers. Use `--no-editor` or
`--no-terminal` to skip one for a particular invocation.

Use `devx pick` to choose an action with `fzf`: open a registered checkout,
create a worktree, set up configuration overlays, or apply them. The direct
commands remain available for scripting. The open picker lists the most
recently/frequently opened projects first, then lets `fzf` fuzzy-filter them.
Choose a project first, then choose its primary checkout or one of its
worktrees. Pickers use compact tabular rows with headers. They show name,
branch, clean/dirty state, and checkout type in narrow terminals; at 100 or more
columns they add the home-relative path. `fzf` filters the visible columns, not
the hidden selection identifier.

## Launchers

`devx init` creates `~/.config/devx/config.toml`. Launchers are token arrays;
`{path}` is replaced with the registered directory. The defaults open IntelliJ
IDEA and Ghostty with `open`:

```toml
[launchers]
editor = ["open", "-a", "IntelliJ IDEA", "{path}"]
terminal = ["open", "-a", "Ghostty", "{path}"]
config_editor = ["open", "-a", "Zed", "{path}"]
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

The command fetches `origin`, resolves its default branch (`main`, `master`,
`develop`, or another configured remote default), creates the new branch from
the current `origin/<default-branch>`, then registers the worktree. Its default
registry name is `my-service-feature-login`; override it with `--name` when
needed.

After creation, `devx` opens the new worktree in the configured editor and
terminal automatically.

The new entry gets `template_project = "my-service"` automatically. It shares
the main project's managed overlays while retaining its own registered name.

Existing worktrees remain where they are. `devx` discovers them through
`git worktree list`, including repository-local `.worktrees` layouts, without
moving or modifying them.

## Managed Configuration Overlays

`devx` keeps local configuration outside repositories. Set up a project once;
with no project name, `fzf` first selects a checkout and then one or more
existing `.yml`, `.yaml`, `.properties`, or `.env` files:

```sh
devx project setup my-service
devx project setup
```

For every selected project file, setup creates empty overlays in both layers
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

A global overlay applies only to projects that have mapped that same relative
path through `project setup`.

Apply every mapped file to a checkout or worktree in one previewed batch:

```sh
devx config list my-service
devx config apply my-service
```

`config apply` merges `base project file < global overlay < project overlay`,
prints every unified diff, and performs no writes unless a single confirmation
is accepted. Relative paths cannot escape their configured roots.

YAML overlays must use nested mappings, not dotted keys. Mappings merge
recursively, scalars replace scalars, and lists replace lists. A mapping/list/
scalar type conflict, duplicate YAML key, or dotted key that overlaps a nested
path stops the entire batch before any file is written. YAML comments and
formatting are normalized after a successful semantic merge.

`.properties` and `.env` overlays merge by key. Later layers replace earlier
values, new keys are appended, and unchanged base lines and comments are kept.
Duplicate keys in the base or either overlay are errors. `.properties` support
is intentionally limited to one logical `key=value` or `key:value` entry per
line; escaped separators, escaped keys, and line continuations are not yet
supported.

`rg` is intentionally not a prerequisite yet. It will be used by a future
configuration content-search command; project and file selection already use
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
cached `devx pick` flow using `[launchers].raycast_terminal` and resolves
`devx` from Raycast's `PATH`, falling back to `~/.cargo/bin/devx`. Its default
is Ghostty; change that launcher to use another terminal.
