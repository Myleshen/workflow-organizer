use std::{
    collections::{BTreeSet, HashMap, HashSet},
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use dialoguer::{Confirm, Input};
use serde_yaml::{Mapping, Value};
use similar::TextDiff;

mod state;

use state::{Config, Launchers, ManagedFile, Paths, Project, ScanRoot, Usage};

#[derive(Parser)]
#[command(about = "macOS project, worktree, and configuration helper")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create the default configuration file if it does not exist.
    Init,
    /// Interactively configure applications, scan roots, cache, and Raycast.
    Setup,
    /// Remove all devx local configuration after confirmation.
    Reset,
    /// Show the bundled manual, including installation and complete workflows.
    Man,
    /// Check local tools, configured applications, and devx setup.
    Doctor,
    /// Generate shell completion scripts to standard output.
    Completions { shell: CompletionShell },
    #[command(subcommand)]
    Raycast(RaycastCommand),
    #[command(subcommand)]
    Launcher(LauncherCommand),
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Open a registered project or worktree in configured applications.
    Open(OpenArgs),
    /// Open or configure the terminal workspace.
    Workspace(WorkspaceArgs),
    /// Interactively select common project, worktree, and configuration actions.
    Pick,
    #[command(subcommand)]
    Worktree(WorktreeCommand),
    #[command(subcommand)]
    Config(ConfigCommand),
}

#[derive(Subcommand)]
enum ProjectCommand {
    /// Register a Git repository or worktree.
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Show all registered projects.
    List,
    /// Remove a registered project.
    Remove { name: String },
    /// Add a directory scanned when the project cache is refreshed.
    AddRoot {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    /// Refresh the cached repository and worktree list from scan roots.
    Refresh,
    /// Clone a repository into a named scan root.
    Clone { root: String, url: String },
    /// Configure overlay files for a project or worktree.
    Setup { project: Option<String> },
    /// Set the overlay owner used by a registered project.
    SetTemplate {
        name: String,
        template_project: String,
    },
}

#[derive(Subcommand)]
enum WorktreeCommand {
    /// Create a worktree from the latest origin default branch.
    Create {
        /// Registered primary project.
        project: String,
        /// New branch name for the worktree.
        branch: String,
        /// Registry name. Defaults to <project>-<branch>.
        #[arg(long)]
        name: Option<String>,
    },
    /// Remove a registered worktree and its Git worktree directory.
    Remove {
        /// Registered worktree name.
        project: String,
        /// Remove even when the worktree has uncommitted changes.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum RaycastCommand {
    /// Install the bundled Raycast Script Command in a local directory.
    Install,
    /// Start the picker in the configured Raycast terminal launcher.
    Pick,
}

#[derive(Subcommand)]
enum LauncherCommand {
    /// Interactively choose configured applications for devx roles.
    Edit,
    /// Print the configured launcher token arrays.
    List,
}

#[derive(Clone, ValueEnum)]
enum CompletionShell {
    Zsh,
    Fish,
}

#[derive(Args)]
struct OpenArgs {
    name: String,
    /// Do not open the configured editor.
    #[arg(long)]
    no_editor: bool,
    /// Do not open the configured terminal.
    #[arg(long)]
    no_terminal: bool,
    /// Use the configured terminal workspace instead of a normal terminal.
    #[arg(long)]
    workspace: bool,
}

#[derive(Args)]
struct WorkspaceArgs {
    /// Project or worktree to open in the workspace.
    name: Option<String>,
    /// Interactively configure the workspace behavior.
    #[arg(long)]
    configure: bool,
}

#[derive(Subcommand)]
enum ConfigCommand {
    /// Preview and apply all configured overlays to a project or worktree.
    Apply { project: String },
    /// Print files configured for a project's overlay layers.
    List { project: String },
    /// Create an empty global overlay file and open the global overlay directory.
    GlobalAdd,
    /// Search mapped base and overlay configuration with ripgrep.
    Search { project: String, query: String },
}

fn default_editor() -> Vec<String> {
    vec![
        "open".into(),
        "-a".into(),
        "IntelliJ IDEA".into(),
        "{path}".into(),
    ]
}

fn default_terminal() -> Vec<String> {
    vec![
        "open".into(),
        "-a".into(),
        "Ghostty".into(),
        "{path}".into(),
    ]
}

fn default_config_editor() -> Vec<String> {
    vec!["open".into(), "-a".into(), "Zed".into(), "{path}".into()]
}

fn default_raycast_terminal() -> Vec<String> {
    vec![
        "open".into(),
        "-a".into(),
        "Ghostty".into(),
        "--args".into(),
        "-e".into(),
        "zsh".into(),
        "-lc".into(),
        "exec {command}".into(),
    ]
}

fn default_vcs() -> Vec<String> {
    vec!["lazygit".into()]
}

fn main() -> Result<()> {
    let paths = Paths::from_environment()?;
    run(Cli::parse().command, &paths)
}

fn run(command: Commands, paths: &Paths) -> Result<()> {
    match command {
        Commands::Init => init(paths),
        Commands::Setup => setup(paths),
        Commands::Reset => reset(paths),
        Commands::Man => show_man_page(),
        Commands::Doctor => doctor(paths),
        Commands::Completions { shell } => completions(shell),
        Commands::Raycast(command) => raycast(command),
        Commands::Launcher(command) => launcher(command, paths),
        Commands::Project(command) => project(command, paths),
        Commands::Open(args) => open(args, paths),
        Commands::Workspace(args) => workspace(args, paths),
        Commands::Pick => pick(paths),
        Commands::Worktree(command) => worktree(command, paths),
        Commands::Config(command) => config(command, paths),
    }
}

fn completions(shell: CompletionShell) -> Result<()> {
    let shell = match shell {
        CompletionShell::Zsh => Shell::Zsh,
        CompletionShell::Fish => Shell::Fish,
    };
    let mut command = Cli::command();
    generate(shell, &mut command, "devx", &mut std::io::stdout());
    Ok(())
}

fn doctor(paths: &Paths) -> Result<()> {
    let config = paths
        .config_file()
        .exists()
        .then(|| load_config(paths))
        .transpose()?;
    let mut failures = 0;
    let mut check = |label: &str, available: bool, hint: &str| {
        if available {
            println!("ok    {label}");
        } else {
            println!("missing {label}\n  {hint}");
            failures += 1;
        }
    };
    check(
        "Git",
        command_exists("git"),
        "install Xcode Command Line Tools",
    );
    check("fzf", command_exists("fzf"), "brew install fzf");
    check_launcher(
        "editor",
        &config
            .as_ref()
            .map_or_else(default_editor, |config| config.launchers.editor.clone()),
        &mut check,
    );
    check_launcher(
        "terminal",
        &config
            .as_ref()
            .map_or_else(default_terminal, |config| config.launchers.terminal.clone()),
        &mut check,
    );
    check_launcher(
        "configuration editor",
        &config
            .as_ref()
            .map_or_else(default_config_editor, |config| {
                config.launchers.config_editor.clone()
            }),
        &mut check,
    );
    check_launcher(
        "Raycast terminal",
        &config
            .as_ref()
            .map_or_else(default_raycast_terminal, |config| {
                config.launchers.raycast_terminal.clone()
            }),
        &mut check,
    );
    if config
        .as_ref()
        .is_none_or(|config| config.workspace.enabled)
    {
        let vcs = config
            .as_ref()
            .map_or_else(default_vcs, |config| config.workspace.vcs.clone());
        let command = vcs.first().context("workspace VCS command is empty")?;
        check(
            "workspace VCS",
            command_exists(command),
            "install the configured VCS tool or set [workspace].enabled = false",
        );
    }
    check(
        "configuration",
        paths.config_file().exists(),
        "run devx init",
    );
    let raycast_script = env::var_os("HOME").map(PathBuf::from).map(|home| {
        home.join("Library/Application Support/Raycast/Script Commands/devx/devx-pick.sh")
    });
    if !raycast_script.as_ref().is_some_and(|path| path.exists()) {
        println!("optional Raycast script\n  Install with: devx raycast install");
    }
    if config
        .as_ref()
        .is_some_and(|config| config.roots.is_empty())
    {
        println!("warning no scan roots\n  Run: devx project add-root <path>");
    }
    if failures == 0 {
        println!("devx is ready.");
        Ok(())
    } else {
        bail!("{failures} required or configured component(s) missing")
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

fn application_exists(application: &str) -> bool {
    Command::new("open")
        .args(["-Ra", application])
        .status()
        .is_ok_and(|status| status.success())
}

fn check_launcher(label: &str, launcher: &[String], check: &mut impl FnMut(&str, bool, &str)) {
    let Some((program, arguments)) = launcher.split_first() else {
        check(
            label,
            false,
            "configure the launcher as a non-empty token array",
        );
        return;
    };
    if program == "open"
        && let Some(application) = arguments
            .windows(2)
            .find_map(|pair| (pair[0] == "-a").then_some(pair[1].as_str()))
    {
        check(
            label,
            application_exists(application),
            "install the configured application or update [launchers]",
        );
        return;
    }
    check(
        label,
        command_exists(program),
        "install the configured command or update [launchers]",
    );
}

fn raycast(command: RaycastCommand) -> Result<()> {
    match command {
        RaycastCommand::Install => install_raycast_script(),
        RaycastCommand::Pick => raycast_pick(),
    }
}

fn raycast_pick() -> Result<()> {
    let paths = Paths::from_environment()?;
    let config = load_config(&paths)?;
    let executable = env::current_exe().context("cannot determine the devx executable path")?;
    let command = format!("exec {} pick", shell_quote(&executable.to_string_lossy()));
    launch_command(
        &config.launchers.raycast_terminal,
        &command,
        "Raycast terminal",
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn install_raycast_script() -> Result<()> {
    let script = raycast_script_path()?;
    let directory = script
        .parent()
        .context("Raycast script path has no parent")?;
    if script.exists()
        && !Confirm::new()
            .with_prompt(format!(
                "Replace existing Raycast script at {}?",
                script.display()
            ))
            .default(false)
            .interact()?
    {
        println!("Raycast script was not changed.");
        return Ok(());
    }
    fs::create_dir_all(directory)?;
    fs::write(&script, include_bytes!("../raycast/devx-pick.sh"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;
    }
    println!("Installed Raycast script\n  {}", script.display());
    println!(
        "\nNext steps\n  1. Raycast Settings > Extensions > Script Commands > Add Directory\n  2. Add the directory above\n  3. Bind Devx Pick to a hotkey"
    );
    Ok(())
}

fn raycast_script_path() -> Result<PathBuf> {
    Ok(
        PathBuf::from(env::var_os("HOME").context("HOME is not set")?)
            .join("Library/Application Support/Raycast/Script Commands/devx/devx-pick.sh"),
    )
}

fn show_man_page() -> Result<()> {
    let manual = env::temp_dir().join(format!("devx-{}.1", std::process::id()));
    fs::write(&manual, include_bytes!("../docs/devx.1"))?;
    let status = Command::new("man")
        .arg(&manual)
        .status()
        .context("could not start man; use 'devx --help' for command help")?;
    fs::remove_file(&manual).ok();
    if status.success() {
        Ok(())
    } else {
        bail!("man failed with status {status}")
    }
}

fn pick(paths: &Paths) -> Result<()> {
    require_fzf()?;
    let Some(action) = select_one(
        "Choose an action",
        &[
            "Open project or worktree".to_owned(),
            "Create worktree".to_owned(),
            "Remove worktree".to_owned(),
            "Set up configuration overlays".to_owned(),
            "Apply configuration overlays".to_owned(),
        ],
    )?
    else {
        return Ok(());
    };

    match action.as_str() {
        "Open project or worktree" => pick_open(paths),
        "Create worktree" => pick_worktree(paths),
        "Remove worktree" => pick_remove_worktree(paths),
        "Set up configuration overlays" => project_setup(None, paths),
        "Apply configuration overlays" => pick_config(paths),
        _ => unreachable!("picker returned an unknown action"),
    }
}

fn pick_open(paths: &Paths) -> Result<()> {
    let mut config = load_config(paths)?;
    let available = available_projects(&config)?;
    let groups = project_groups(&available, &config.usage);
    let Some(repository) = select_table(
        "Choose a project",
        "PROJECT\tCHECKOUTS",
        &groups
            .iter()
            .map(|group| format!("{}\t{}\t{}", group.name, group.name, group.projects.len()))
            .collect::<Vec<_>>(),
    )?
    else {
        return Ok(());
    };
    let group = groups
        .iter()
        .find(|group| group.name == repository)
        .context("selected project group is no longer available")?;
    let mut checkouts = group.projects.clone();
    checkouts.sort_by(|left, right| checkout_sort_key(left).cmp(&checkout_sort_key(right)));
    let Some(name) = select_project("Choose a checkout", &checkouts)? else {
        return Ok(());
    };
    open(
        OpenArgs {
            name: name.clone(),
            no_editor: false,
            no_terminal: false,
            workspace: config.workspace.enabled,
        },
        paths,
    )?;
    record_open(&mut config, &name, paths)
}

struct ProjectGroup<'a> {
    name: String,
    projects: Vec<&'a Project>,
    last_opened: u64,
    opens: u64,
}

fn project_groups<'a>(
    projects: &'a [Project],
    usage: &HashMap<String, Usage>,
) -> Vec<ProjectGroup<'a>> {
    let mut groups: HashMap<String, ProjectGroup<'a>> = HashMap::new();
    for project in projects {
        let group = groups
            .entry(project.template_name().to_owned())
            .or_insert_with(|| ProjectGroup {
                name: project.template_name().to_owned(),
                projects: Vec::new(),
                last_opened: 0,
                opens: 0,
            });
        let project_usage = usage.get(&project.name).cloned().unwrap_or_default();
        group.last_opened = group.last_opened.max(project_usage.last_opened);
        group.opens += project_usage.opens;
        group.projects.push(project);
    }
    let mut groups: Vec<_> = groups.into_values().collect();
    groups.sort_by(|left, right| {
        right
            .last_opened
            .cmp(&left.last_opened)
            .then_with(|| right.opens.cmp(&left.opens))
            .then_with(|| left.name.cmp(&right.name))
    });
    groups
}

fn checkout_sort_key(project: &Project) -> (u8, &str) {
    (
        u8::from(project.is_worktree),
        project.branch.as_deref().unwrap_or(""),
    )
}

fn pick_worktree(paths: &Paths) -> Result<()> {
    let config = load_config(paths)?;
    let available = available_projects(&config)?;
    let primary_projects: Vec<_> = available
        .iter()
        .filter(|project| project.template_project.is_none())
        .collect();
    let Some(project) = select_project("Choose a primary project", &primary_projects)? else {
        return Ok(());
    };
    let branch: String = Input::new()
        .with_prompt("New branch name")
        .interact_text()?;
    worktree(
        WorktreeCommand::Create {
            project,
            branch,
            name: None,
        },
        paths,
    )
}

fn pick_remove_worktree(paths: &Paths) -> Result<()> {
    let config = load_config(paths)?;
    let available = available_projects(&config)?;
    let worktrees: Vec<_> = available
        .iter()
        .filter(|project| project.is_worktree)
        .collect();
    let Some(name) = select_project("Choose a worktree to remove", &worktrees)? else {
        return Ok(());
    };
    let selected = get_project(&available, &name)?;
    let dirty = is_dirty(&selected.path);
    let prompt = if dirty {
        format!("'{name}' is dirty. Force-remove it and discard uncommitted changes?")
    } else {
        format!("Remove worktree '{name}'?")
    };
    if !Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()?
    {
        println!("Worktree was not removed.");
        return Ok(());
    }
    worktree_remove(&name, dirty, paths)
}

fn pick_config(paths: &Paths) -> Result<()> {
    let config = load_config(paths)?;
    let available = available_projects(&config)?;
    let projects: Vec<_> = available.iter().collect();
    let Some(project) = select_project("Choose a project", &projects)? else {
        return Ok(());
    };
    config_apply(paths, &project)
}

fn init(paths: &Paths) -> Result<()> {
    if paths.config_file().exists() {
        println!(
            "Configuration already exists\n  {}",
            paths.config_file().display()
        );
        return Ok(());
    }
    save_config(paths, &Config::default())?;
    println!("Created configuration\n  {}", paths.config_file().display());
    Ok(())
}

fn setup(paths: &Paths) -> Result<()> {
    let mut config = if paths.config_file().exists() {
        load_config(paths)?
    } else {
        Config::default()
    };
    configure_launchers(&mut config)?;
    configure_roots(&mut config)?;
    refresh_project_cache(&mut config)?;
    save_config(paths, &config)?;
    println!(
        "Configured project cache\n  {} project(s)",
        config.cached_projects.len()
    );
    if Confirm::new()
        .with_prompt("Install the Raycast Devx Pick command?")
        .default(false)
        .interact()?
    {
        install_raycast_script()?;
    }
    println!("\nSetup complete\n  Run: devx doctor");
    print_recommended_tools(&config);
    Ok(())
}

fn print_recommended_tools(config: &Config) {
    let mut missing = Vec::new();
    if !application_exists("Ghostty") {
        missing.push("Ghostty: brew install --cask ghostty");
    }
    if !application_exists("Zed") {
        missing.push("Zed: brew install --cask zed");
    }
    if config.workspace.enabled && !command_exists("lazygit") {
        missing.push("LazyGit: brew install lazygit");
    }
    if missing.is_empty() {
        return;
    }
    println!("\nOptional recommended tools");
    for tool in missing {
        println!("  {tool}");
    }
}

fn reset(paths: &Paths) -> Result<()> {
    if !paths.config_dir.exists() {
        println!(
            "No devx configuration exists\n  {}",
            paths.config_dir.display()
        );
        return Ok(());
    }
    if !Confirm::new()
        .with_prompt(format!(
            "Remove all devx configuration and overlays from {}? Repositories will not be changed",
            paths.config_dir.display()
        ))
        .default(false)
        .interact()?
    {
        println!("Configuration was not removed.");
        return Ok(());
    }
    fs::remove_dir_all(&paths.config_dir)?;
    println!("Removed devx configuration.");
    Ok(())
}

fn launcher(command: LauncherCommand, paths: &Paths) -> Result<()> {
    let mut config = load_config(paths)?;
    match command {
        LauncherCommand::Edit => {
            configure_launchers(&mut config)?;
            save_config(paths, &config)?;
            println!("Updated launchers.");
        }
        LauncherCommand::List => print_launchers(&config.launchers),
    }
    Ok(())
}

fn configure_launchers(config: &mut Config) -> Result<()> {
    let applications = discover_applications()?;
    choose_launcher(
        "IDE/editor",
        &applications,
        &mut config.launchers.editor,
        false,
    )?;
    choose_launcher(
        "terminal",
        &applications,
        &mut config.launchers.terminal,
        false,
    )?;
    choose_launcher(
        "configuration editor",
        &applications,
        &mut config.launchers.config_editor,
        false,
    )?;
    choose_launcher(
        "Raycast terminal",
        &applications,
        &mut config.launchers.raycast_terminal,
        true,
    )
}

fn configure_roots(config: &mut Config) -> Result<()> {
    loop {
        let path: String = Input::new()
            .with_prompt("Scan root to add (example: ~/dev; Enter finishes)")
            .allow_empty(true)
            .interact_text()?;
        if path.trim().is_empty() {
            break;
        }
        let path = fs::canonicalize(expand_home_path(path.trim())?)?;
        if !path.is_dir() {
            bail!("{} is not a directory", path.display());
        }
        if config.roots.iter().any(|root| root.path == path) {
            println!("Already configured\n  {}", path.display());
            continue;
        }
        let default_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let name: String = Input::new()
            .with_prompt(format!("Scan root name (Enter uses {default_name})"))
            .allow_empty(true)
            .interact_text()?;
        let name = if name.trim().is_empty() {
            default_name
        } else {
            name
        };
        if config.roots.iter().any(|root| root.name == name) {
            bail!("a root named '{name}' is already configured");
        }
        config.roots.push(ScanRoot { name, path });
    }
    config
        .roots
        .sort_by(|left, right| left.name.cmp(&right.name));
    Ok(())
}

fn expand_home_path(path: &str) -> Result<PathBuf> {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME is not set");
    }
    if let Some(path) = path.strip_prefix("~/") {
        let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
        return Ok(home.join(path));
    }
    Ok(PathBuf::from(path))
}

fn discover_applications() -> Result<Vec<String>> {
    let mut applications = BTreeSet::new();
    let mut directories = vec![PathBuf::from("/Applications")];
    if let Some(home) = env::var_os("HOME") {
        directories.push(PathBuf::from(home).join("Applications"));
    }
    if let Ok(output) = Command::new("brew")
        .args(["--prefix", "--caskroom"])
        .output()
        && output.status.success()
    {
        let caskroom = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        if caskroom.is_dir() {
            for cask in fs::read_dir(caskroom)? {
                let cask = cask?.path();
                if let Some(version) = fs::read_dir(&cask)?.next() {
                    directories.push(version?.path());
                }
            }
        }
    }
    for directory in directories {
        collect_applications(&directory, &mut applications)?;
    }
    Ok(applications.into_iter().collect())
}

fn collect_applications(directory: &Path, applications: &mut BTreeSet<String>) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "app")
            && let Some(name) = path.file_stem()
        {
            applications.insert(name.to_string_lossy().into_owned());
        }
    }
    Ok(())
}

fn choose_launcher(
    role: &str,
    applications: &[String],
    launcher: &mut Vec<String>,
    raycast: bool,
) -> Result<()> {
    let current = launcher_application(launcher).unwrap_or("custom");
    let mut choices = applications.to_vec();
    choices.push("Keep current".into());
    let Some(choice) = select_one(&format!("Choose {role} (current: {current})"), &choices)? else {
        return Ok(());
    };
    if choice == "Keep current" {
        return Ok(());
    }
    *launcher = if raycast {
        vec![
            "open".into(),
            "-a".into(),
            choice,
            "--args".into(),
            "-e".into(),
            "zsh".into(),
            "-lc".into(),
            "exec {command}".into(),
        ]
    } else {
        vec!["open".into(), "-a".into(), choice, "{path}".into()]
    };
    Ok(())
}

fn launcher_application(launcher: &[String]) -> Option<&str> {
    launcher
        .windows(2)
        .find_map(|pair| (pair[0] == "-a").then_some(pair[1].as_str()))
}

fn print_launchers(launchers: &Launchers) {
    for (role, launcher) in [
        ("editor", &launchers.editor),
        ("terminal", &launchers.terminal),
        ("configuration editor", &launchers.config_editor),
        ("Raycast terminal", &launchers.raycast_terminal),
    ] {
        println!("{role}\n  {}\n", launcher.join(" "));
    }
}

fn project(command: ProjectCommand, paths: &Paths) -> Result<()> {
    let mut config = load_config(paths)?;
    match command {
        ProjectCommand::Add { path, name } => {
            let path = fs::canonicalize(&path)
                .with_context(|| format!("cannot access {}", path.display()))?;
            if !path.join(".git").exists() {
                bail!("{} is not a Git repository or worktree", path.display());
            }
            let name = name.unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            if !project_name_available(&config, &name)? {
                bail!(
                    "a project named '{name}' is already registered; use --name to choose an alias"
                );
            }
            config.projects.push(Project {
                name: name.clone(),
                path,
                template_project: None,
                branch: None,
                is_worktree: false,
            });
            config
                .projects
                .sort_by(|left, right| left.name.cmp(&right.name));
            save_config(paths, &config)?;
            println!("Registered {name}");
        }
        ProjectCommand::List => {
            for project in available_projects(&config)? {
                println!(
                    "{}\t{}\t{}",
                    project.name,
                    project.path.display(),
                    project.template_name()
                );
            }
        }
        ProjectCommand::Remove { name } => {
            let count = config.projects.len();
            config.projects.retain(|project| project.name != name);
            if config.projects.len() == count {
                bail!("no project named '{name}' is registered");
            }
            save_config(paths, &config)?;
            println!("Removed {name}");
        }
        ProjectCommand::AddRoot { path, name } => {
            let path = fs::canonicalize(&path)
                .with_context(|| format!("cannot access {}", path.display()))?;
            if !path.is_dir() {
                bail!("{} is not a directory", path.display());
            }
            let name = name.unwrap_or_else(|| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
            if config.roots.iter().any(|root| root.name == name) {
                bail!("a root named '{name}' is already registered");
            }
            config.roots.push(ScanRoot {
                name: name.clone(),
                path,
            });
            refresh_project_cache(&mut config)?;
            config
                .roots
                .sort_by(|left, right| left.name.cmp(&right.name));
            save_config(paths, &config)?;
            println!("Added scan root {name}");
        }
        ProjectCommand::Refresh => {
            refresh_project_cache(&mut config)?;
            save_config(paths, &config)?;
            println!(
                "Refreshed {} cached project(s)",
                config.cached_projects.len()
            );
        }
        ProjectCommand::Clone { root, url } => {
            clone_project(&config, &root, &url)?;
            refresh_project_cache(&mut config)?;
            save_config(paths, &config)?;
        }
        ProjectCommand::Setup { project } => return project_setup(project, paths),
        ProjectCommand::SetTemplate {
            name,
            template_project,
        } => {
            let available = available_projects(&config)?;
            let template_project = get_project(&available, &template_project)?
                .template_name()
                .to_owned();
            let project = config
                .projects
                .iter_mut()
                .find(|project| project.name == name)
                .with_context(|| format!("no project named '{name}' is registered"))?;
            project.template_project = (name != template_project).then_some(template_project);
            save_config(paths, &config)?;
            println!("Updated template project for {name}");
        }
    }
    Ok(())
}

fn clone_project(config: &Config, root_name: &str, url: &str) -> Result<()> {
    let root = config
        .roots
        .iter()
        .find(|root| root.name == root_name)
        .with_context(|| format!("no scan root named '{root_name}' is registered"))?;
    let repository_name = repository_name_from_url(url)?;
    let destination = root.path.join(&repository_name);
    if destination.exists() {
        bail!("destination already exists: {}", destination.display());
    }
    let destination = destination.to_string_lossy();
    run_command("git", ["clone", url, &destination], None)?;
    println!("Cloned {repository_name}\n  {}", root.path.display());
    Ok(())
}

fn repository_name_from_url(url: &str) -> Result<String> {
    let repository = url
        .trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .unwrap_or_default()
        .strip_suffix(".git")
        .unwrap_or_default();
    if repository.is_empty() || repository == "." || repository == ".." {
        bail!("cannot derive a repository name from '{url}'");
    }
    Ok(repository.to_owned())
}

fn available_projects(config: &Config) -> Result<Vec<Project>> {
    let mut projects = config.projects.clone();
    projects.extend(config.cached_projects.clone());
    finalize_projects(projects)
}

fn project_name_available(config: &Config, name: &str) -> Result<bool> {
    Ok(!available_projects(config)?
        .iter()
        .any(|project| project.name == name))
}

fn refresh_project_cache(config: &mut Config) -> Result<()> {
    config.cached_projects = expand_worktrees(discover_projects(&config.roots)?)?;
    let registered_paths: HashSet<_> = config
        .projects
        .iter()
        .map(|project| project.path.clone())
        .collect();
    config
        .cached_projects
        .retain(|project| !registered_paths.contains(&project.path));
    config
        .cached_projects
        .sort_by(|left, right| left.name.cmp(&right.name));
    config.cache_initialized = true;
    Ok(())
}

fn finalize_projects(mut projects: Vec<Project>) -> Result<Vec<Project>> {
    projects.sort_by(|left, right| left.name.cmp(&right.name));

    let mut names = HashMap::new();
    for project in &projects {
        if let Some(previous) = names.insert(&project.name, &project.path) {
            bail!(
                "project name collision for '{}': {} and {}; register one with 'devx project add <path> --name <alias>'",
                project.name,
                previous.display(),
                project.path.display()
            );
        }
    }
    Ok(projects)
}

fn expand_worktrees(projects: Vec<Project>) -> Result<Vec<Project>> {
    let known_paths: HashSet<_> = projects
        .iter()
        .map(|project| project.path.clone())
        .collect();
    let primary_projects = projects.clone();
    let mut expanded = projects;
    let mut seen_paths = known_paths.clone();
    for primary in &primary_projects {
        for worktree in git_worktrees(&primary.path)? {
            if !seen_paths.insert(worktree.path.clone()) {
                continue;
            }
            let name = worktree
                .path
                .file_name()
                .context("worktree has no directory name")?
                .to_string_lossy()
                .into_owned();
            expanded.push(Project {
                name,
                path: worktree.path,
                template_project: Some(primary.template_name().to_owned()),
                branch: worktree.branch,
                is_worktree: true,
            });
        }
    }
    for project in &mut expanded {
        if project.branch.is_none() {
            project.branch = git_branch(&project.path);
        }
    }
    rename_collisions(&mut expanded);
    Ok(expanded)
}

struct GitWorktree {
    path: PathBuf,
    branch: Option<String>,
}

fn git_worktrees(repository: &Path) -> Result<Vec<GitWorktree>> {
    let output = git_output(repository, ["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();
    for block in output.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        for line in block.lines() {
            if let Some(value) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(value));
            }
            if let Some(value) = line.strip_prefix("branch refs/heads/") {
                branch = Some(value.to_owned());
            }
        }
        if let Some(path) = path {
            worktrees.push(GitWorktree { path, branch });
        }
    }
    Ok(worktrees)
}

fn git_branch(repository: &Path) -> Option<String> {
    git_output(repository, ["branch", "--show-current"])
        .ok()
        .filter(|branch| !branch.is_empty())
}

fn rename_collisions(projects: &mut [Project]) {
    let mut names = HashMap::new();
    for project in projects.iter() {
        *names.entry(project.name.clone()).or_insert(0_usize) += 1;
    }
    for project in projects.iter_mut() {
        if names[&project.name] > 1 {
            let context = if project.is_worktree {
                project.template_name().to_owned()
            } else {
                project_context(&project.path)
            };
            project.name = format!("{context}-{}", project.name);
        }
    }
}

fn project_context(path: &Path) -> String {
    let parent = path.parent();
    let directory = parent.and_then(Path::file_name).unwrap_or_default();
    if directory == ".worktrees" {
        return parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
    }
    directory.to_string_lossy().into_owned()
}

fn discover_projects(roots: &[ScanRoot]) -> Result<Vec<Project>> {
    let mut discovered = Vec::new();
    for root in roots {
        discover_projects_in(&root.path, root, &mut discovered)?;
    }

    let mut basename_counts = HashMap::new();
    for (project, _) in &discovered {
        *basename_counts
            .entry(project.name.clone())
            .or_insert(0_usize) += 1;
    }
    for (project, root_name) in &mut discovered {
        if basename_counts[&project.name] > 1 {
            project.name = format!("{root_name}-{}", project.name);
        }
    }
    Ok(discovered.into_iter().map(|(project, _)| project).collect())
}

fn discover_projects_in(
    directory: &Path,
    root: &ScanRoot,
    projects: &mut Vec<(Project, String)>,
) -> Result<()> {
    if is_git_checkout(directory) {
        let name = directory
            .file_name()
            .context("Git checkout has no directory name")?
            .to_string_lossy()
            .into_owned();
        projects.push((
            Project {
                name,
                path: directory.to_owned(),
                template_project: None,
                branch: None,
                is_worktree: false,
            },
            root.name.clone(),
        ));
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == ".git") || !path.is_dir() {
            continue;
        }
        discover_projects_in(&path, root, projects)?;
    }
    Ok(())
}

fn is_git_checkout(path: &Path) -> bool {
    path.join(".git").exists()
}

fn worktree(command: WorktreeCommand, paths: &Paths) -> Result<()> {
    match command {
        WorktreeCommand::Create {
            project,
            branch,
            name,
        } => worktree_create(project, branch, name, paths),
        WorktreeCommand::Remove { project, force } => worktree_remove(&project, force, paths),
    }
}

fn worktree_create(
    project: String,
    branch: String,
    name: Option<String>,
    paths: &Paths,
) -> Result<()> {
    let mut config = load_config(paths)?;
    let available = available_projects(&config)?;
    let primary = get_project(&available, &project)?;
    if primary.template_project.is_some() {
        bail!("'{project}' is a worktree; create new worktrees from its primary project instead");
    }
    let root = primary
        .path
        .parent()
        .context("registered project has no parent directory")?
        .join(".worktrees")
        .join(&primary.name);
    validate_branch(&primary.path, &branch)?;
    let worktree_name = name.unwrap_or_else(|| default_worktree_name(&project, &branch));
    if !project_name_available(&config, &worktree_name)? {
        bail!(
            "a project named '{worktree_name}' is already registered; use --name to choose an alias"
        );
    }
    let destination = root.join(worktree_directory_name(&branch));
    if destination.exists() {
        bail!("worktree path already exists: {}", destination.display());
    }

    let default_branch = fetch_default_branch(&primary.path)?;
    run_git(
        &primary.path,
        [
            "worktree",
            "add",
            "-b",
            &branch,
            &destination.to_string_lossy(),
            &format!("origin/{default_branch}"),
        ],
    )?;
    config.projects.push(Project {
        name: worktree_name.clone(),
        path: destination.clone(),
        template_project: Some(primary.template_name().to_owned()),
        branch: Some(branch),
        is_worktree: true,
    });
    config
        .projects
        .sort_by(|left, right| left.name.cmp(&right.name));
    save_config(paths, &config)?;
    launch(&config.launchers.editor, &destination, "editor")?;
    launch(&config.launchers.terminal, &destination, "terminal")?;
    println!(
        "Created and registered {worktree_name}\n  {}",
        destination.display()
    );
    Ok(())
}

fn worktree_remove(name: &str, force: bool, paths: &Paths) -> Result<()> {
    let mut config = load_config(paths)?;
    let available = available_projects(&config)?;
    let worktree = get_project(&available, name)?;
    if !worktree.is_worktree {
        bail!("'{name}' is not a worktree; devx only removes worktrees");
    }
    if is_dirty(&worktree.path) && !force {
        bail!("'{name}' has uncommitted changes; rerun with --force to remove it");
    }
    let primary_name = worktree.template_name();
    let primary = available
        .iter()
        .find(|project| project.name == primary_name && !project.is_worktree)
        .with_context(|| format!("cannot find primary project '{primary_name}'"))?;
    let mut arguments = vec!["worktree", "remove"];
    if force {
        arguments.push("--force");
    }
    arguments.push(
        worktree
            .path
            .to_str()
            .context("worktree path is not UTF-8")?,
    );
    run_git_vec(&primary.path, &arguments)?;
    config
        .projects
        .retain(|project| project.path != worktree.path);
    config
        .cached_projects
        .retain(|project| project.path != worktree.path);
    save_config(paths, &config)?;
    println!("Removed worktree {name}\n  {}", worktree.path.display());
    Ok(())
}

fn open(args: OpenArgs, paths: &Paths) -> Result<()> {
    let config = load_config(paths)?;
    let projects = available_projects(&config)?;
    let project = get_project(&projects, &args.name)?;
    if !args.no_editor {
        launch(&config.launchers.editor, &project.path, "editor")?;
    }
    if !args.no_terminal {
        if args.workspace && config.workspace.enabled {
            launch_workspace(&config, project)?;
        } else {
            launch(&config.launchers.terminal, &project.path, "terminal")?;
        }
    }
    Ok(())
}

fn workspace(args: WorkspaceArgs, paths: &Paths) -> Result<()> {
    let mut config = load_config(paths)?;
    if args.configure {
        configure_workspace(&mut config)?;
        save_config(paths, &config)?;
        println!("Updated workspace settings.");
        return Ok(());
    }
    let name = args
        .name
        .context("provide a project name or use --configure")?;
    let projects = available_projects(&config)?;
    let project = get_project(&projects, &name)?;
    launch_workspace(&config, project)
}

fn configure_workspace(config: &mut Config) -> Result<()> {
    let enabled = Confirm::new()
        .with_prompt("Use the terminal workspace by default from devx pick?")
        .default(config.workspace.enabled)
        .interact()?;
    config.workspace.enabled = enabled;
    if !enabled {
        println!("Picker selections will use normal terminal opens.");
        return Ok(());
    }
    let current = config.workspace.vcs.join(" ");
    let command: String = Input::new()
        .with_prompt("VCS command for the workspace")
        .default(current)
        .interact_text()?;
    let command: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
    if command.is_empty() {
        bail!("workspace VCS command cannot be empty");
    }
    config.workspace.vcs = command;
    let terminal = launcher_application(&config.launchers.terminal).unwrap_or("custom");
    if terminal == "Ghostty" {
        println!("Ghostty workspace\n  Native split: shell left, VCS right.");
    } else if command_exists("tmux")
        && config
            .launchers
            .terminal
            .iter()
            .any(|argument| argument.contains("{command}"))
    {
        println!("{terminal} workspace\n  tmux split: shell left, VCS right.");
    } else {
        println!(
            "{terminal} workspace\n  Normal terminal fallback.\n  Add {{command}} to its launcher and install tmux for splits."
        );
    }
    Ok(())
}

fn launch_workspace(config: &Config, project: &Project) -> Result<()> {
    if !config.workspace.enabled {
        return launch(&config.launchers.terminal, &project.path, "terminal");
    }
    let vcs = config
        .workspace
        .vcs
        .first()
        .context("workspace VCS command is empty")?;
    if !command_exists(vcs) {
        println!("Workspace VCS is unavailable\n  {vcs}\n\nOpening a normal terminal instead.");
        return launch(&config.launchers.terminal, &project.path, "terminal");
    }
    if launcher_application(&config.launchers.terminal) == Some("Ghostty") {
        return launch_ghostty_workspace(&project.path, &vcs_command(&config.workspace.vcs));
    }
    if command_exists("tmux")
        && config
            .launchers
            .terminal
            .iter()
            .any(|argument| argument.contains("{command}"))
    {
        return launch_tmux_workspace(
            &config.launchers.terminal,
            &project.path,
            &vcs_command(&config.workspace.vcs),
        );
    }
    println!(
        "Terminal workspace is unavailable\n  Configure a terminal launcher with {{command}} to use tmux.\n\nOpening a normal terminal instead."
    );
    launch(&config.launchers.terminal, &project.path, "terminal")
}

fn vcs_command(vcs: &[String]) -> String {
    vcs.iter()
        .map(|argument| shell_quote(argument))
        .collect::<Vec<_>>()
        .join(" ")
}

fn launch_ghostty_workspace(path: &Path, vcs: &str) -> Result<()> {
    let working_directory = apple_script_quote(&path.to_string_lossy());
    let command = apple_script_quote(&format!("exec {vcs}"));
    let script = format!(
        "tell application \"Ghostty\"\n\
           set config to new surface configuration\n\
           set initial to new window with configuration {{initial working directory:{working_directory}}}\n\
           split (focused terminal of selected tab of initial) direction right with configuration {{initial working directory:{working_directory}, command:{command}}}\n\
           activate\n\
         end tell"
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("could not start osascript for Ghostty workspace")?;
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "Ghostty workspace failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

fn launch_tmux_workspace(terminal: &[String], path: &Path, vcs: &str) -> Result<()> {
    let command = format!(
        "tmux new-session -A -s devx -c {} \\; split-window -h -c {} '{}' \\; select-pane -L",
        shell_quote(&path.to_string_lossy()),
        shell_quote(&path.to_string_lossy()),
        vcs
    );
    launch_command(terminal, &command, "terminal workspace")
}

fn apple_script_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn config(command: ConfigCommand, paths: &Paths) -> Result<()> {
    let app = load_config(paths)?;
    match command {
        ConfigCommand::Apply { project } => config_apply(paths, &project),
        ConfigCommand::Search { project, query } => config_search(paths, &project, &query),
        ConfigCommand::List { project } => {
            let projects = available_projects(&app)?;
            let registered = get_project(&projects, &project)?;
            let mut files: Vec<_> = app
                .managed_files
                .iter()
                .filter(|file| file.project == registered.template_name())
                .map(|file| file.destination.clone())
                .collect();
            files.sort();
            if files.is_empty() {
                println!(
                    "No overlay files configured\n  {}",
                    registered.template_name()
                );
            } else {
                for file in files {
                    println!("{}", file.display());
                }
            }
            Ok(())
        }
        ConfigCommand::GlobalAdd => config_global_add(paths),
    }
}

fn config_search(paths: &Paths, project: &str, query: &str) -> Result<()> {
    let config = load_config(paths)?;
    let projects = available_projects(&config)?;
    let registered = get_project(&projects, project)?;
    let files: Vec<_> = config
        .managed_files
        .iter()
        .filter(|file| file.project == registered.template_name())
        .flat_map(|file| {
            [
                registered.path.join(&file.destination),
                paths.global_overlays_dir().join(&file.destination),
                paths.overlays_dir(&file.project).join(&file.destination),
            ]
        })
        .filter(|path| path.exists())
        .collect();
    if files.is_empty() {
        bail!("no mapped configuration files found for {project}");
    }
    let output = Command::new("rg")
        .args([
            "--ignore-case",
            "--line-number",
            "--with-filename",
            "--color",
            "always",
            query,
        ])
        .args(&files)
        .output()
        .context("could not start rg; install it with 'brew install ripgrep'")?;
    if output.status.code() == Some(1) {
        println!("No matches for '{query}'.");
        return Ok(());
    }
    if !output.status.success() {
        bail!(
            "rg failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}

fn config_apply(paths: &Paths, project: &str) -> Result<()> {
    let config = load_config(paths)?;
    let projects = available_projects(&config)?;
    let registered = get_project(&projects, project)?;
    let managed: Vec<_> = config
        .managed_files
        .iter()
        .filter(|file| file.project == registered.template_name())
        .collect();
    if managed.is_empty() {
        bail!(
            "no overlay files configured for {}; run 'devx project setup {project}'",
            registered.template_name()
        );
    }

    let checkout_root = fs::canonicalize(&registered.path)
        .with_context(|| format!("cannot access checkout {}", registered.path.display()))?;
    let mut changes = Vec::new();
    for file in managed {
        let destination = registered.path.join(&file.destination);
        ensure_within_checkout(&checkout_root, &destination, &registered.path)?;
        let old = fs::read_to_string(&destination)
            .with_context(|| format!("cannot read base configuration {}", destination.display()))?;
        let global = paths.global_overlays_dir().join(&file.destination);
        let project_overlay = paths.overlays_dir(&file.project).join(&file.destination);
        let new = merge_config_file(&destination, &old, &global, &project_overlay)?;
        if new != old {
            let diff = TextDiff::from_lines(&old, &new)
                .unified_diff()
                .header(&destination.display().to_string(), "merged overlay")
                .to_string();
            changes.push((destination, old, new, diff));
        }
    }
    if changes.is_empty() {
        println!("No configuration changes.");
        return Ok(());
    }
    for (_, _, _, diff) in &changes {
        print!("{diff}");
    }
    if !Confirm::new()
        .with_prompt(format!("Apply {} configuration change(s)?", changes.len()))
        .default(false)
        .interact()?
    {
        println!("No files changed.");
        return Ok(());
    }
    write_config_batch(changes)?;
    Ok(())
}

fn ensure_within_checkout(checkout_root: &Path, destination: &Path, checkout: &Path) -> Result<()> {
    let canonical_destination = fs::canonicalize(destination)
        .with_context(|| format!("cannot access base configuration {}", destination.display()))?;
    if canonical_destination.starts_with(checkout_root) {
        Ok(())
    } else {
        bail!(
            "managed configuration {} resolves outside checkout {}",
            destination.display(),
            checkout.display()
        );
    }
}

fn write_config_batch(changes: Vec<(PathBuf, String, String, String)>) -> Result<()> {
    let mut staged = Vec::new();
    for (index, (destination, old, new, _)) in changes.into_iter().enumerate() {
        let temporary =
            destination.with_file_name(format!(".devx-{}-{index}.tmp", std::process::id()));
        fs::write(&temporary, new)
            .with_context(|| format!("cannot stage configuration {}", destination.display()))?;
        staged.push((destination, old, temporary));
    }

    let mut applied = Vec::new();
    for (destination, old, temporary) in staged {
        if let Err(error) = fs::rename(&temporary, &destination) {
            for (destination, old) in applied {
                fs::write(destination, old).ok();
            }
            fs::remove_file(&temporary).ok();
            bail!(
                "cannot apply configuration {}: {error}",
                destination.display()
            );
        }
        println!("Updated\n  {}", destination.display());
        applied.push((destination, old));
    }
    Ok(())
}

fn project_setup(project: Option<String>, paths: &Paths) -> Result<()> {
    require_fzf()?;
    let mut config = load_config(paths)?;
    let available = available_projects(&config)?;
    let project = match project {
        Some(project) => project,
        None => {
            let projects: Vec<_> = available.iter().collect();
            let Some(project) = select_project("Choose a project", &projects)? else {
                return Ok(());
            };
            project
        }
    };
    let selected = get_project(&available, &project)?;
    let candidates = discover_config_files(&selected.path)?;
    if candidates.is_empty() {
        bail!(
            "no supported configuration files found in {}",
            selected.path.display()
        );
    }
    let choices: Vec<_> = candidates
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let selected_files = select_many("Select files to manage", &choices)?;
    if selected_files.is_empty() {
        return Ok(());
    }
    let owner = selected.template_name().to_owned();
    for selected_file in selected_files {
        let destination = PathBuf::from(selected_file);
        validate_relative(&destination, "configuration file")?;
        ensure_supported_config_file(&destination)?;
        let managed = ManagedFile {
            project: owner.clone(),
            destination: destination.clone(),
        };
        if !config.managed_files.contains(&managed) {
            config.managed_files.push(managed);
        }
        create_empty_overlay(&paths.global_overlays_dir().join(&destination))?;
        create_empty_overlay(&paths.overlays_dir(&owner).join(&destination))?;
    }
    config.managed_files.sort_by(|left, right| {
        left.project
            .cmp(&right.project)
            .then_with(|| left.destination.cmp(&right.destination))
    });
    save_config(paths, &config)?;
    launch(
        &config.launchers.config_editor,
        &paths.overlays_dir(&owner),
        "configuration editor",
    )?;
    println!(
        "Configured overlays\n  Owner: {owner}\n  Directory: {}",
        paths.overlays_dir(&owner).display()
    );
    Ok(())
}

fn config_global_add(paths: &Paths) -> Result<()> {
    let config = load_config(paths)?;
    let filename: String = Input::new()
        .with_prompt("Global overlay path relative to the project root")
        .interact_text()?;
    let filename = PathBuf::from(filename.trim());
    validate_relative(&filename, "global overlay path")?;
    ensure_supported_config_file(&filename)?;
    let overlay = paths.global_overlays_dir().join(&filename);
    create_empty_overlay(&overlay)?;
    launch(
        &config.launchers.config_editor,
        &paths.global_overlays_dir(),
        "configuration editor",
    )?;
    println!("Created global overlay\n  {}", overlay.display());
    Ok(())
}

fn create_empty_overlay(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    fs::create_dir_all(path.parent().context("overlay path has no parent")?)?;
    fs::write(path, "")?;
    Ok(())
}

fn discover_config_files(project: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_config_files(project, project, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_config_files(root: &Path, current: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(is_ignored_config_directory) {
                continue;
            }
            collect_config_files(root, &path, files)?;
        } else if is_supported_config_file(&path) {
            files.push(path.strip_prefix(root)?.to_owned());
        }
    }
    Ok(())
}

fn is_ignored_config_directory(name: &std::ffi::OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".git" | ".worktrees" | ".idea" | "target" | "build" | "node_modules")
    )
}

fn ensure_supported_config_file(path: &Path) -> Result<()> {
    if is_supported_config_file(path) {
        Ok(())
    } else {
        bail!(
            "{} must end in .yml, .yaml, .properties, or .env",
            path.display()
        )
    }
}

fn is_supported_config_file(path: &Path) -> bool {
    if path.file_name().is_some_and(|name| name == ".env") {
        return true;
    }
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yml" | "yaml" | "properties" | "env")
    )
}

fn merge_config_file(
    destination: &Path,
    base: &str,
    global: &Path,
    project: &Path,
) -> Result<String> {
    match destination
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("yml" | "yaml") => merge_yaml(base, global, project),
        Some("properties") => merge_key_values(base, global, project, KeyValueFormat::Properties),
        Some("env") | None if destination.file_name().is_some_and(|name| name == ".env") => {
            merge_key_values(base, global, project, KeyValueFormat::Env)
        }
        _ => bail!(
            "unsupported configuration file type: {}",
            destination.display()
        ),
    }
}

fn read_optional_overlay(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)
        .with_context(|| format!("cannot read overlay {}", path.display()))?;
    Ok((!content.trim().is_empty()).then_some(content))
}

fn merge_yaml(base: &str, global_path: &Path, project_path: &Path) -> Result<String> {
    let overlays = [
        read_optional_overlay(global_path)?,
        read_optional_overlay(project_path)?,
    ];
    if overlays.iter().all(Option::is_none) {
        return Ok(base.to_owned());
    }
    let mut merged = parse_yaml_mapping(base, "base configuration")?;
    for (path, overlay) in [global_path, project_path].into_iter().zip(overlays) {
        let Some(overlay) = overlay else {
            continue;
        };
        let overlay = parse_yaml_mapping(&overlay, &format!("overlay {}", path.display()))?;
        validate_yaml_overlay(&merged, &overlay)?;
        merge_yaml_value(&mut merged, overlay, "")?;
    }
    serde_yaml::to_string(&merged).context("cannot render merged YAML")
}

fn parse_yaml_mapping(content: &str, label: &str) -> Result<Value> {
    reject_duplicate_yaml_keys(content, label)?;
    let value: Value =
        serde_yaml::from_str(content).with_context(|| format!("invalid YAML in {label}"))?;
    if !matches!(value, Value::Mapping(_)) {
        bail!("{label} must contain a YAML mapping at its root");
    }
    Ok(value)
}

fn reject_duplicate_yaml_keys(content: &str, label: &str) -> Result<()> {
    let mut mappings: Vec<(usize, HashSet<String>)> = Vec::new();
    for line in content.lines() {
        let indentation = line.len() - line.trim_start().len();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with('-') {
            mappings.retain(|(indent, _)| *indent < indentation);
            continue;
        }
        let Some((key, _)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(' ') || key.starts_with(['\'', '"']) {
            continue;
        }
        mappings.retain(|(indent, _)| *indent <= indentation);
        if mappings
            .last()
            .is_none_or(|(indent, _)| *indent != indentation)
        {
            mappings.push((indentation, HashSet::new()));
        }
        if !mappings
            .last_mut()
            .expect("mapping stack is populated")
            .1
            .insert(key.to_owned())
        {
            bail!("duplicate YAML key '{key}' in {label}");
        }
    }
    Ok(())
}

fn validate_yaml_overlay(base: &Value, overlay: &Value) -> Result<()> {
    let mut overlay_paths = BTreeSet::new();
    collect_overlay_paths(overlay, &mut Vec::new(), &mut overlay_paths)?;
    let mut base_dotted_paths = BTreeSet::new();
    collect_base_dotted_paths(base, &mut Vec::new(), &mut base_dotted_paths)?;
    for overlay_path in &overlay_paths {
        for base_path in &base_dotted_paths {
            if paths_overlap(overlay_path, base_path) {
                bail!(
                    "YAML overlay path '{}' conflicts with dotted base key '{}'; use a non-overlapping configuration structure",
                    overlay_path.join("."),
                    base_path.join(".")
                );
            }
        }
    }
    Ok(())
}

fn collect_overlay_paths(
    value: &Value,
    prefix: &mut Vec<String>,
    paths: &mut BTreeSet<Vec<String>>,
) -> Result<()> {
    let Value::Mapping(mapping) = value else {
        return Ok(());
    };
    for (key, value) in mapping {
        let Value::String(key) = key else {
            bail!("YAML overlay keys must be strings");
        };
        if key.contains('.') {
            bail!("YAML overlay key '{key}' is dotted; use nested mappings instead");
        }
        prefix.push(key.clone());
        paths.insert(prefix.clone());
        collect_overlay_paths(value, prefix, paths)?;
        prefix.pop();
    }
    Ok(())
}

fn collect_base_dotted_paths(
    value: &Value,
    prefix: &mut Vec<String>,
    paths: &mut BTreeSet<Vec<String>>,
) -> Result<()> {
    let Value::Mapping(mapping) = value else {
        return Ok(());
    };
    for (key, value) in mapping {
        let Value::String(key) = key else {
            continue;
        };
        let parts: Vec<_> = key.split('.').map(str::to_owned).collect();
        let dotted = parts.len() > 1;
        prefix.extend(parts);
        if dotted {
            paths.insert(prefix.clone());
        }
        collect_base_dotted_paths(value, prefix, paths)?;
        for _ in 0..key.split('.').count() {
            prefix.pop();
        }
    }
    Ok(())
}

fn paths_overlap(left: &[String], right: &[String]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn merge_yaml_value(base: &mut Value, overlay: Value, path: &str) -> Result<()> {
    match (base, overlay) {
        (Value::Mapping(base), Value::Mapping(overlay)) => merge_yaml_mapping(base, overlay, path),
        (base, overlay) if yaml_kind(base) == yaml_kind(&overlay) => {
            *base = overlay;
            Ok(())
        }
        (base, overlay) => bail!(
            "YAML type conflict at {}: base is {}, overlay is {}",
            if path.is_empty() { "root" } else { path },
            yaml_kind(base),
            yaml_kind(&overlay)
        ),
    }
}

fn merge_yaml_mapping(base: &mut Mapping, overlay: Mapping, path: &str) -> Result<()> {
    for (key, overlay_value) in overlay {
        let Value::String(key_name) = &key else {
            bail!("YAML overlay keys must be strings");
        };
        let child_path = if path.is_empty() {
            key_name.clone()
        } else {
            format!("{path}.{key_name}")
        };
        match base.get_mut(&key) {
            Some(base_value) => merge_yaml_value(base_value, overlay_value, &child_path)?,
            None => {
                base.insert(key, overlay_value);
            }
        }
    }
    Ok(())
}

fn yaml_kind(value: &Value) -> &'static str {
    match value {
        Value::Mapping(_) => "mapping",
        Value::Sequence(_) => "list",
        _ => "scalar",
    }
}

#[derive(Clone, Copy)]
enum KeyValueFormat {
    Properties,
    Env,
}

fn merge_key_values(
    base: &str,
    global_path: &Path,
    project_path: &Path,
    format: KeyValueFormat,
) -> Result<String> {
    let overlays = [
        read_optional_overlay(global_path)?,
        read_optional_overlay(project_path)?,
    ];
    if overlays.iter().all(Option::is_none) {
        return Ok(base.to_owned());
    }
    let mut document = KeyValueDocument::parse(base, format, "base configuration")?;
    for (path, overlay) in [global_path, project_path].into_iter().zip(overlays) {
        let Some(overlay) = overlay else {
            continue;
        };
        let overlay =
            KeyValueDocument::parse(&overlay, format, &format!("overlay {}", path.display()))?;
        document.apply(overlay);
    }
    Ok(document.render())
}

#[derive(Debug)]
struct KeyValueDocument {
    lines: Vec<String>,
    keys: HashMap<String, usize>,
    values: HashMap<String, String>,
    key_order: Vec<String>,
    trailing_newline: bool,
}

impl KeyValueDocument {
    fn parse(content: &str, format: KeyValueFormat, label: &str) -> Result<Self> {
        let mut document = Self {
            lines: content.lines().map(str::to_owned).collect(),
            keys: HashMap::new(),
            values: HashMap::new(),
            key_order: Vec::new(),
            trailing_newline: content.ends_with('\n'),
        };
        for (index, line) in document.lines.iter().enumerate() {
            if let Some((key, value, _)) = parse_key_value(line, format, label)? {
                if document.keys.insert(key.clone(), index).is_some() {
                    bail!("duplicate key '{key}' in {label}");
                }
                document.key_order.push(key.clone());
                document.values.insert(key, value);
            }
        }
        Ok(document)
    }

    fn apply(&mut self, overlay: Self) {
        for key in overlay.key_order {
            let value = overlay.values[&key].clone();
            if let Some(index) = self.keys.get(&key) {
                self.lines[*index] = format!("{}={value}", key);
            } else {
                self.keys.insert(key.clone(), self.lines.len());
                self.lines.push(format!("{key}={value}"));
            }
        }
    }

    fn render(self) -> String {
        let mut rendered = self.lines.join("\n");
        if self.trailing_newline {
            rendered.push('\n');
        }
        rendered
    }
}

fn parse_key_value(
    line: &str,
    format: KeyValueFormat,
    label: &str,
) -> Result<Option<(String, String, usize)>> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || matches!(format, KeyValueFormat::Properties) && trimmed.starts_with('!')
    {
        return Ok(None);
    }
    let separator = match format {
        KeyValueFormat::Properties => line.find(['=', ':']),
        KeyValueFormat::Env => line.find('='),
    }
    .with_context(|| format!("invalid key/value entry '{line}' in {label}"))?;
    let key = line[..separator].trim();
    if key.is_empty() {
        bail!("empty key in {label}");
    }
    Ok(Some((
        key.to_owned(),
        line[separator + 1..].trim().to_owned(),
        separator,
    )))
}

impl Project {
    fn template_name(&self) -> &str {
        self.template_project.as_deref().unwrap_or(&self.name)
    }
}

fn require_fzf() -> Result<()> {
    if Command::new("fzf").arg("--version").output().is_ok() {
        return Ok(());
    }
    bail!("'devx pick' requires fzf; install it with 'brew install fzf'")
}

fn select_project(title: &str, projects: &[&Project]) -> Result<Option<String>> {
    let wide = terminal_columns() >= 100;
    let header = if wide {
        "NAME\tBRANCH\tSTATE\tTYPE\tPATH"
    } else {
        "NAME\tBRANCH\tSTATE\tTYPE"
    };
    let choices: Vec<_> = projects
        .iter()
        .map(|project| project_picker_entry(project, wide))
        .collect();
    select_table(title, header, &choices)
}

fn project_picker_entry(project: &&Project, wide: bool) -> String {
    let branch = project.branch.as_deref().unwrap_or("detached");
    let state = if is_dirty(&project.path) {
        "dirty"
    } else {
        "clean"
    };
    let kind = if project.is_worktree {
        "worktree"
    } else {
        "repo"
    };
    let row = format!(
        "{}\t{}\t{}\t{}\t{}",
        project.name,
        branch,
        state,
        kind,
        display_path(&project.path)
    );
    if wide {
        format!("{}\t{row}", project.name)
    } else {
        format!(
            "{}\t{}\t{}\t{}\t{}",
            project.name, project.name, branch, state, kind
        )
    }
}

fn display_path(path: &Path) -> String {
    env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|home| {
            path.strip_prefix(home)
                .ok()
                .map(|path| format!("~/{}", path.display()))
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn record_open(config: &mut Config, name: &str, paths: &Paths) -> Result<()> {
    let usage = config.usage.entry(name.to_owned()).or_default();
    usage.opens += 1;
    usage.last_opened = unix_timestamp()?;
    save_config(paths, config)
}

fn unix_timestamp() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .context("system clock is before Unix epoch")
        .map(|duration| duration.as_secs())
}

fn is_dirty(path: &Path) -> bool {
    git_output(path, ["status", "--porcelain"])
        .map(|output| !output.is_empty())
        .unwrap_or(true)
}

fn select_one(prompt: &str, choices: &[String]) -> Result<Option<String>> {
    let mut child = Command::new("fzf")
        .args([
            "--prompt",
            &format!("{prompt}> "),
            "--height",
            "~40%",
            "--ignore-case",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("could not start fzf")?;
    let input = choices.join("\n");
    child
        .stdin
        .as_mut()
        .context("could not open fzf input")?
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    if output.status.code() == Some(130) || output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!("fzf failed with status {}", output.status);
    }
    let selection = String::from_utf8(output.stdout).context("fzf returned non-UTF-8 output")?;
    Ok(Some(selection.trim_end().to_owned()))
}

fn select_table(prompt: &str, header: &str, rows: &[String]) -> Result<Option<String>> {
    let mut child = Command::new("fzf")
        .args([
            "--prompt",
            &format!("{prompt}> "),
            "--height",
            "~70%",
            "--ignore-case",
            "--delimiter",
            "\t",
            "--with-nth",
            "2..",
            "--header",
            header,
            "--tabstop",
            "2",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("could not start fzf")?;
    child
        .stdin
        .as_mut()
        .context("could not open fzf input")?
        .write_all(rows.join("\n").as_bytes())?;
    let output = child.wait_with_output()?;
    if output.status.code() == Some(130) || output.status.code() == Some(1) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!("fzf failed with status {}", output.status);
    }
    let selection = String::from_utf8(output.stdout).context("fzf returned non-UTF-8 output")?;
    Ok(selection
        .trim_end()
        .split_once('\t')
        .map(|(identifier, _)| identifier.to_owned()))
}

fn terminal_columns() -> u16 {
    Command::new("tput")
        .arg("cols")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|columns| columns.trim().parse().ok())
        .unwrap_or(80)
}

fn select_many(prompt: &str, choices: &[String]) -> Result<Vec<String>> {
    let mut child = Command::new("fzf")
        .args([
            "--prompt",
            &format!("{prompt}> "),
            "--height",
            "~40%",
            "--multi",
            "--ignore-case",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("could not start fzf")?;
    let input = choices.join("\n");
    child
        .stdin
        .as_mut()
        .context("could not open fzf input")?
        .write_all(input.as_bytes())?;
    let output = child.wait_with_output()?;
    if output.status.code() == Some(130) || output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        bail!("fzf failed with status {}", output.status);
    }
    String::from_utf8(output.stdout)
        .context("fzf returned non-UTF-8 output")
        .map(|selection| selection.lines().map(str::to_owned).collect())
}

fn launch(template: &[String], path: &Path, kind: &str) -> Result<()> {
    launch_command(template, &path.to_string_lossy(), kind)
}

fn launch_command(template: &[String], command: &str, kind: &str) -> Result<()> {
    let (program, arguments) = template
        .split_first()
        .with_context(|| format!("{kind} launcher is empty"))?;
    let arguments: Vec<_> = arguments
        .iter()
        .map(|argument| {
            argument
                .replace("{path}", command)
                .replace("{command}", command)
        })
        .collect();
    Command::new(program)
        .args(arguments)
        .spawn()
        .with_context(|| format!("cannot launch configured {kind} command"))?;
    Ok(())
}

fn fetch_default_branch(repository: &Path) -> Result<String> {
    run_git(repository, ["fetch", "--prune", "origin"])?;
    let reference = git_output(
        repository,
        [
            "symbolic-ref",
            "--quiet",
            "--short",
            "refs/remotes/origin/HEAD",
        ],
    )?;
    reference
        .strip_prefix("origin/")
        .map(str::to_owned)
        .context(
        "origin has no default branch; run 'git remote set-head origin -a' in the primary checkout",
    )
}

fn validate_branch(repository: &Path, branch: &str) -> Result<()> {
    run_git(repository, ["check-ref-format", "--branch", branch])
}

fn default_worktree_name(project: &str, branch: &str) -> String {
    format!("{project}-{}", branch.replace('/', "-"))
}

fn worktree_directory_name(branch: &str) -> String {
    branch.replace('/', "-")
}

fn run_git<const N: usize>(repository: &Path, arguments: [&str; N]) -> Result<()> {
    run_git_vec(repository, &arguments)
}

fn run_git_vec(repository: &Path, arguments: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .context("could not start git")?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "git failed in {}: {}",
        repository.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn run_command<const N: usize>(
    program: &str,
    arguments: [&str; N],
    directory: Option<&Path>,
) -> Result<()> {
    let mut command = Command::new(program);
    command.args(arguments);
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let output = command
        .output()
        .with_context(|| format!("could not start {program}"))?;
    if output.status.success() {
        return Ok(());
    }
    bail!(
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn git_output<const N: usize>(repository: &Path, arguments: [&str; N]) -> Result<String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .context("could not start git")?;
    if !output.status.success() {
        bail!(
            "git failed in {}: {}",
            repository.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout)
        .context("git returned non-UTF-8 output")
        .map(|output| output.trim().to_owned())
}

fn get_project<'a>(projects: &'a [Project], name: &str) -> Result<&'a Project> {
    projects
        .iter()
        .find(|project| project.name == name)
        .with_context(|| format!("no project named '{name}' is registered"))
}

fn validate_relative(path: &Path, label: &str) -> Result<()> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        bail!("{label} must be a relative path without '..'");
    }
    Ok(())
}

fn load_config(paths: &Paths) -> Result<Config> {
    let file = paths.config_file();
    if !file.exists() {
        bail!("configuration not found; run 'devx init'");
    }
    let mut config: Config = toml::from_str(&fs::read_to_string(&file)?)
        .with_context(|| format!("invalid configuration in {}", file.display()))?;
    if !config.cache_initialized {
        refresh_project_cache(&mut config)?;
        save_config(paths, &config)?;
    }
    Ok(config)
}

fn save_config(paths: &Paths, config: &Config) -> Result<()> {
    fs::create_dir_all(&paths.config_dir)?;
    fs::write(paths.config_file(), toml::to_string_pretty(config)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn rejects_unsafe_relative_paths() {
        assert!(validate_relative(Path::new("bootstrap.yml"), "template").is_ok());
        assert!(validate_relative(Path::new("../bootstrap.yml"), "template").is_err());
        assert!(validate_relative(Path::new("/tmp/bootstrap.yml"), "template").is_err());
    }

    #[test]
    fn saves_and_loads_config() {
        let directory = tempdir().unwrap();
        let paths = Paths {
            config_dir: directory.path().join("devx"),
        };
        let config = Config {
            projects: vec![Project {
                name: "api".into(),
                path: PathBuf::from("/tmp/api"),
                template_project: None,
                branch: None,
                is_worktree: false,
            }],
            roots: Vec::new(),
            usage: HashMap::new(),
            launchers: Launchers::default(),
            workspace: Default::default(),
            managed_files: Vec::new(),
            cached_projects: Vec::new(),
            cache_initialized: false,
        };
        save_config(&paths, &config).unwrap();
        let loaded = load_config(&paths).unwrap();
        assert_eq!(loaded.projects[0].name, "api");
        assert_eq!(loaded.projects[0].path, PathBuf::from("/tmp/api"));
    }

    #[test]
    fn worktree_name_replaces_branch_separators() {
        assert_eq!(
            default_worktree_name("api", "feature/login"),
            "api-feature-login"
        );
        assert_eq!(worktree_directory_name("feature/login"), "feature-login");
    }

    #[test]
    fn prefixes_every_discovered_name_in_a_collision() {
        let directory = tempdir().unwrap();
        let dev = directory.path().join("dev");
        let learning = directory.path().join("learning");
        fs::create_dir_all(dev.join("payments/.git")).unwrap();
        fs::create_dir_all(learning.join("payments/.git")).unwrap();
        let projects = discover_projects(&[
            ScanRoot {
                name: "dev".into(),
                path: dev,
            },
            ScanRoot {
                name: "learning".into(),
                path: learning,
            },
        ])
        .unwrap();
        let names: Vec<_> = projects.into_iter().map(|project| project.name).collect();
        assert_eq!(names, ["dev-payments", "learning-payments"]);
    }

    #[test]
    fn identifies_worktrees_by_primary_repository() {
        assert_eq!(
            project_context(Path::new("/dev/api/.worktrees/feature")),
            "api"
        );
    }

    #[test]
    fn merges_yaml_overlays_in_layer_order() {
        let directory = tempdir().unwrap();
        let global = directory.path().join("global.yml");
        let project = directory.path().join("project.yml");
        fs::write(
            &global,
            "spring:\n  config:\n    import: global\n  application:\n    name: global\n",
        )
        .unwrap();
        fs::write(&project, "spring:\n  application:\n    name: project\n").unwrap();

        let merged = merge_yaml(
            "spring:\n  application:\n    name: base\n  main:\n    banner-mode: off\n",
            &global,
            &project,
        )
        .unwrap();
        let value: Value = serde_yaml::from_str(&merged).unwrap();
        let expected: Value = serde_yaml::from_str(
            "spring:\n  application:\n    name: project\n  main:\n    banner-mode: off\n  config:\n    import: global\n",
        )
        .unwrap();
        assert_eq!(value, expected);
    }

    #[test]
    fn rejects_dotted_yaml_overlay_keys() {
        let directory = tempdir().unwrap();
        let global = directory.path().join("global.yml");
        fs::write(&global, "spring.application.name: local\n").unwrap();
        let error = merge_yaml(
            "spring: {}\n",
            &global,
            &directory.path().join("missing.yml"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("dotted"));
    }

    #[test]
    fn rejects_duplicate_yaml_keys() {
        let error = parse_yaml_mapping("spring: one\nspring: two\n", "overlay").unwrap_err();
        assert!(error.to_string().contains("duplicate"));
    }

    #[test]
    fn rejects_yaml_type_conflicts() {
        let directory = tempdir().unwrap();
        let global = directory.path().join("global.yml");
        fs::write(&global, "spring: local\n").unwrap();
        let error = merge_yaml(
            "spring:\n  application: api\n",
            &global,
            &directory.path().join("missing.yml"),
        )
        .unwrap_err();
        assert!(error.to_string().contains("type conflict"));
    }

    #[test]
    fn merges_properties_without_rewriting_unmodified_lines() {
        let directory = tempdir().unwrap();
        let global = directory.path().join("global.properties");
        let project = directory.path().join("project.properties");
        fs::write(&global, "host=global\nnew=value\n").unwrap();
        fs::write(&project, "host=project\n").unwrap();
        let merged = merge_key_values(
            "# keep this comment\nhost=base\nport:8080\n",
            &global,
            &project,
            KeyValueFormat::Properties,
        )
        .unwrap();
        assert_eq!(
            merged,
            "# keep this comment\nhost=project\nport:8080\nnew=value\n"
        );
    }

    #[test]
    fn rejects_duplicate_key_value_entries() {
        let error = KeyValueDocument::parse("HOST=one\nHOST=two\n", KeyValueFormat::Env, "overlay")
            .unwrap_err();
        assert!(error.to_string().contains("duplicate key 'HOST'"));
    }

    #[test]
    fn creates_empty_overlay_under_requested_directory() {
        let directory = tempdir().unwrap();
        let overlay = directory
            .path()
            .join("configs/api/src/main/resources/bootstrap.yml");
        create_empty_overlay(&overlay).unwrap();
        assert_eq!(fs::read_to_string(overlay).unwrap(), "");
    }

    #[test]
    fn uses_cached_projects_without_running_git() {
        let config = Config {
            cached_projects: vec![Project {
                name: "cached".into(),
                path: PathBuf::from("/not/a/repository"),
                template_project: None,
                branch: None,
                is_worktree: false,
            }],
            ..Config::default()
        };
        assert_eq!(available_projects(&config).unwrap()[0].name, "cached");
    }

    #[test]
    fn rejects_name_collisions_with_cached_projects() {
        let config = Config {
            cached_projects: vec![Project {
                name: "cached".into(),
                path: PathBuf::from("/tmp/cached"),
                template_project: None,
                branch: None,
                is_worktree: false,
            }],
            ..Config::default()
        };
        assert!(!project_name_available(&config, "cached").unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinked_config_outside_checkout() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let checkout = directory.path().join("checkout");
        let outside = directory.path().join("outside.yml");
        fs::create_dir_all(&checkout).unwrap();
        fs::write(&outside, "key: value\n").unwrap();
        symlink(&outside, checkout.join("config.yml")).unwrap();
        let error = ensure_within_checkout(
            &fs::canonicalize(&checkout).unwrap(),
            &checkout.join("config.yml"),
            &checkout,
        )
        .unwrap_err();
        assert!(error.to_string().contains("outside checkout"));
    }

    #[test]
    fn collects_app_bundle_names() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join("Zed.app")).unwrap();
        fs::create_dir_all(directory.path().join("Ghostty.app")).unwrap();
        fs::write(directory.path().join("notes.txt"), "not an app").unwrap();
        let mut applications = BTreeSet::new();
        collect_applications(directory.path(), &mut applications).unwrap();
        assert_eq!(
            applications.into_iter().collect::<Vec<_>>(),
            ["Ghostty", "Zed"]
        );
    }

    #[test]
    fn extracts_configured_application_name() {
        assert_eq!(launcher_application(&default_terminal()), Some("Ghostty"));
    }

    #[test]
    fn expands_tilde_scan_root_input() {
        let home = PathBuf::from(env::var_os("HOME").unwrap());
        assert_eq!(expand_home_path("~").unwrap(), home);
        assert_eq!(expand_home_path("~/dev").unwrap(), home.join("dev"));
    }

    #[test]
    fn uses_directory_name_when_scan_root_alias_is_empty() {
        let default_name = "dev".to_owned();
        let entered = "".to_owned();
        let name = if entered.trim().is_empty() {
            default_name
        } else {
            entered
        };
        assert_eq!(name, "dev");
    }

    #[test]
    fn formats_compact_project_picker_row_without_path() {
        let project = Project {
            name: "api".into(),
            path: PathBuf::from("/Users/example/dev/api"),
            template_project: None,
            branch: Some("main".into()),
            is_worktree: false,
        };
        assert_eq!(
            project_picker_entry(&&project, false),
            "api\tapi\tmain\tdirty\trepo"
        );
    }

    #[test]
    fn formats_wide_project_picker_row_with_path() {
        let project = Project {
            name: "api".into(),
            path: PathBuf::from("/Users/example/dev/api"),
            template_project: None,
            branch: Some("main".into()),
            is_worktree: false,
        };
        assert!(project_picker_entry(&&project, true).ends_with("\t/Users/example/dev/api"));
    }

    #[test]
    fn worktree_remove_refuses_dirty_without_force() {
        let directory = tempdir().unwrap();
        fs::create_dir_all(directory.path().join(".git")).unwrap();
        fs::write(directory.path().join("changed.txt"), "changed").unwrap();
        let project = Project {
            name: "feature".into(),
            path: directory.path().to_owned(),
            template_project: Some("primary".into()),
            branch: Some("feature".into()),
            is_worktree: true,
        };
        assert!(project.is_worktree);
        assert!(is_dirty(&project.path));
    }

    #[test]
    fn config_search_includes_base_and_both_overlay_layers() {
        let paths = Paths {
            config_dir: PathBuf::from("/tmp/devx"),
        };
        let project = Project {
            name: "api".into(),
            path: PathBuf::from("/tmp/api"),
            template_project: None,
            branch: None,
            is_worktree: false,
        };
        let file = ManagedFile {
            project: "api".into(),
            destination: PathBuf::from("src/bootstrap.yml"),
        };
        let candidates = [
            project.path.join(&file.destination),
            paths.global_overlays_dir().join(&file.destination),
            paths.overlays_dir(&file.project).join(&file.destination),
        ];
        assert_eq!(candidates[0], PathBuf::from("/tmp/api/src/bootstrap.yml"));
        assert_eq!(
            candidates[1],
            PathBuf::from("/tmp/devx/configs/global/src/bootstrap.yml")
        );
        assert_eq!(
            candidates[2],
            PathBuf::from("/tmp/devx/configs/api/src/bootstrap.yml")
        );
    }

    #[test]
    fn workspace_defaults_to_enabled_lazygit() {
        let config = Config::default();
        assert!(config.workspace.enabled);
        assert_eq!(config.workspace.vcs, ["lazygit"]);
    }

    #[test]
    fn quotes_vcs_tokens_for_workspace_shell_commands() {
        assert_eq!(vcs_command(&["git".into(), "gui".into()]), "'git' 'gui'");
    }
}
