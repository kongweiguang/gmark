// @author kongweiguang

//! Production update harness.
//!
//! The application and updater are deliberately treated as external programs here. The
//! harness only owns an isolated configuration/update root and a small marker protocol;
//! this keeps the test usable while the in-process updater API evolves.

mod runner;
pub use runner::parse_ack_version;

use std::env;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_TIMEOUT: Duration = Duration::from_secs(60 * 60);
const E2E_DIR: &str = ".gmark-updater-e2e";

/// The decision made by the application when unsaved work is present.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnsavedDecision {
    Cancel,
    Save,
    Discard,
}

impl UnsavedDecision {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Save => "save",
            Self::Discard => "discard",
        }
    }

    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "cancel" => Ok(Self::Cancel),
            "save" => Ok(Self::Save),
            "discard" | "drop" => Ok(Self::Discard),
            _ => Err(format!(
                "invalid --decision '{value}'; expected cancel, save, or discard"
            )),
        }
    }
}

/// Observable expectations for an unsaved-document branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecisionPlan {
    pub continue_install: bool,
    pub helper_must_not_start: bool,
    pub old_process_must_exit: bool,
}

/// Returns the branch contract independently of process/filesystem effects.
#[must_use]
pub fn decision_plan(decision: UnsavedDecision) -> DecisionPlan {
    match decision {
        UnsavedDecision::Cancel => DecisionPlan {
            continue_install: false,
            helper_must_not_start: true,
            old_process_must_exit: false,
        },
        UnsavedDecision::Save | UnsavedDecision::Discard => DecisionPlan {
            continue_install: true,
            helper_must_not_start: false,
            old_process_must_exit: true,
        },
    }
}

/// Parsed command-line options. Paths remain supplied values until resolve_paths applies
/// workspace-relative resolution and isolation checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdaterE2eOptions {
    pub config_root: Option<PathBuf>,
    pub updates_root: Option<PathBuf>,
    pub current_binary: Option<PathBuf>,
    pub next_binary: Option<PathBuf>,
    pub current_installer: Option<PathBuf>,
    pub next_installer: Option<PathBuf>,
    pub signing_private_key: Option<PathBuf>,
    pub signing_public_key: Option<PathBuf>,
    pub public_key_base64: Option<String>,
    pub current_version: Option<String>,
    pub target_version: Option<String>,
    pub manifest_url: Option<String>,
    pub driver: Option<PathBuf>,
    pub helper: Option<PathBuf>,
    pub agent: Option<PathBuf>,
    pub apply_plan: Option<PathBuf>,
    pub acknowledgement: Option<PathBuf>,
    pub version_marker: Option<PathBuf>,
    pub backup: Option<PathBuf>,
    pub installer_log: Option<PathBuf>,
    pub old_pid: Option<PathBuf>,
    pub new_pid: Option<PathBuf>,
    pub helper_pid: Option<PathBuf>,
    pub agent_pid: Option<PathBuf>,
    pub result: Option<PathBuf>,
    pub timeout: Duration,
    pub decision: UnsavedDecision,
    pub dry_run: bool,
    pub fixture: bool,
    pub keep_temp: bool,
}

impl Default for UpdaterE2eOptions {
    fn default() -> Self {
        Self {
            config_root: None,
            updates_root: None,
            current_binary: None,
            next_binary: None,
            current_installer: None,
            next_installer: None,
            signing_private_key: None,
            signing_public_key: None,
            public_key_base64: None,
            current_version: None,
            target_version: None,
            manifest_url: None,
            driver: None,
            helper: None,
            agent: None,
            apply_plan: None,
            acknowledgement: None,
            version_marker: None,
            backup: None,
            installer_log: None,
            old_pid: None,
            new_pid: None,
            helper_pid: None,
            agent_pid: None,
            result: None,
            timeout: DEFAULT_TIMEOUT,
            decision: UnsavedDecision::Save,
            dry_run: false,
            fixture: false,
            keep_temp: false,
        }
    }
}

/// Result of parsing updater arguments. help is separated so callers can return zero without
/// constructing an incomplete production run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedUpdaterE2eArgs {
    pub options: UpdaterE2eOptions,
    pub help: bool,
}

/// Parses updater-e2e arguments without touching the filesystem.
pub fn parse_args(arguments: &[String]) -> Result<ParsedUpdaterE2eArgs, String> {
    let mut options = UpdaterE2eOptions::default();
    let mut help = false;
    let mut index = 0;
    while let Some(argument) = arguments.get(index) {
        if argument == "--help" || argument == "-h" {
            help = true;
            index += 1;
            continue;
        }
        match argument.as_str() {
            "--dry-run" => options.dry_run = true,
            "--fixture" => options.fixture = true,
            "--keep-temp" => options.keep_temp = true,
            "--config-root" | "--config-dir" => {
                options.config_root = Some(path_value(arguments, &mut index, argument)?)
            }
            "--updates-root" | "--update-root" | "--updates-dir" => {
                options.updates_root = Some(path_value(arguments, &mut index, argument)?)
            }
            "--current-binary" | "--binary-n" | "--n-binary" => {
                options.current_binary = Some(path_value(arguments, &mut index, argument)?)
            }
            "--next-binary" | "--binary-n-plus-one" | "--n-plus-one-binary" => {
                options.next_binary = Some(path_value(arguments, &mut index, argument)?)
            }
            "--current-installer" | "--installer-n" | "--n-installer" => {
                options.current_installer = Some(path_value(arguments, &mut index, argument)?)
            }
            "--next-installer" | "--installer-n-plus-one" | "--n-plus-one-installer" => {
                options.next_installer = Some(path_value(arguments, &mut index, argument)?)
            }
            "--signing-private-key" | "--test-signing-private-key" | "--private-key" => {
                options.signing_private_key = Some(path_value(arguments, &mut index, argument)?)
            }
            "--signing-public-key" | "--test-signing-public-key" | "--public-key" => {
                options.signing_public_key = Some(path_value(arguments, &mut index, argument)?)
            }
            "--public-key-base64" => {
                options.public_key_base64 = Some(string_value(arguments, &mut index, argument)?)
            }
            "--current-version" | "--version-n" | "--n-version" => {
                options.current_version = Some(string_value(arguments, &mut index, argument)?)
            }
            "--target-version" | "--version-n-plus-one" | "--n-plus-one-version" => {
                options.target_version = Some(string_value(arguments, &mut index, argument)?)
            }
            "--manifest-url" => {
                options.manifest_url = Some(string_value(arguments, &mut index, argument)?)
            }
            "--driver" | "--harness" => {
                options.driver = Some(path_value(arguments, &mut index, argument)?)
            }
            "--helper" | "--helper-binary" => {
                options.helper = Some(path_value(arguments, &mut index, argument)?)
            }
            "--agent" | "--agent-binary" => {
                options.agent = Some(path_value(arguments, &mut index, argument)?)
            }
            "--apply-plan" => {
                options.apply_plan = Some(path_value(arguments, &mut index, argument)?)
            }
            "--ack" | "--acknowledgement" | "--ack-path" => {
                options.acknowledgement = Some(path_value(arguments, &mut index, argument)?)
            }
            "--version-marker" | "--version-path" => {
                options.version_marker = Some(path_value(arguments, &mut index, argument)?)
            }
            "--backup" | "--backup-path" => {
                options.backup = Some(path_value(arguments, &mut index, argument)?)
            }
            "--installer-log" => {
                options.installer_log = Some(path_value(arguments, &mut index, argument)?)
            }
            "--old-pid" | "--old-pid-file" => {
                options.old_pid = Some(path_value(arguments, &mut index, argument)?)
            }
            "--new-pid" | "--new-pid-file" => {
                options.new_pid = Some(path_value(arguments, &mut index, argument)?)
            }
            "--helper-pid" | "--helper-pid-file" => {
                options.helper_pid = Some(path_value(arguments, &mut index, argument)?)
            }
            "--agent-pid" | "--agent-pid-file" => {
                options.agent_pid = Some(path_value(arguments, &mut index, argument)?)
            }
            "--result" | "--result-path" => {
                options.result = Some(path_value(arguments, &mut index, argument)?)
            }
            "--timeout" => {
                let value = string_value(arguments, &mut index, argument)?;
                options.timeout = parse_timeout(&value)?;
            }
            "--decision" => {
                let value = string_value(arguments, &mut index, argument)?;
                options.decision = UnsavedDecision::parse(&value)?;
            }
            unknown => {
                return Err(format!(
                    "unknown updater-e2e option '{unknown}'; use --help for the harness contract"
                ));
            }
        }
        index += 1;
    }
    Ok(ParsedUpdaterE2eArgs { options, help })
}

fn path_value(arguments: &[String], index: &mut usize, option: &str) -> Result<PathBuf, String> {
    string_value(arguments, index, option).map(PathBuf::from)
}

fn string_value(arguments: &[String], index: &mut usize, option: &str) -> Result<String, String> {
    let value_index = index.saturating_add(1);
    let value = arguments
        .get(value_index)
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(|| format!("{option} requires a value"))?;
    *index = value_index;
    Ok(value.clone())
}

/// Parses bounded timeouts such as 5s, 500ms, or 2m.
pub fn parse_timeout(value: &str) -> Result<Duration, String> {
    let (number, multiplier) = if let Some(value) = value.strip_suffix("ms") {
        (value, 1_u64)
    } else if let Some(value) = value.strip_suffix('s') {
        (value, 1_000)
    } else if let Some(value) = value.strip_suffix('m') {
        (value, 60_000)
    } else {
        (value, 1_000)
    };
    let number = number
        .parse::<u64>()
        .map_err(|_| format!("invalid timeout '{value}'; use a positive duration"))?;
    if number == 0 {
        return Err("timeout must be greater than zero".to_owned());
    }
    let millis = number
        .checked_mul(multiplier)
        .ok_or_else(|| "timeout is too large".to_owned())?;
    let timeout = Duration::from_millis(millis);
    if timeout > MAX_TIMEOUT {
        return Err(format!("timeout exceeds {} seconds", MAX_TIMEOUT.as_secs()));
    }
    Ok(timeout)
}

fn valid_version(value: &str) -> bool {
    semver::Version::parse(value).is_ok()
}

/// Compares the full SemVer precedence, including prerelease identifiers.
/// This is required for the first production updater closure where
/// `0.1.8-rc.1` legitimately upgrades to `0.1.8` without inventing `0.1.9`.
#[must_use]
pub fn version_is_newer(current: &str, target: &str) -> bool {
    let Ok(current) = semver::Version::parse(current) else {
        return false;
    };
    let Ok(target) = semver::Version::parse(target) else {
        return false;
    };
    target > current
}

/// All marker and log paths used by a run. Marker paths are always inside updates_root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct E2ePaths {
    pub temporary_root: Option<PathBuf>,
    pub config_root: PathBuf,
    pub updates_root: PathBuf,
    pub control_root: PathBuf,
    pub logs_root: PathBuf,
    pub acknowledgement: PathBuf,
    pub version_marker: PathBuf,
    pub backup: PathBuf,
    pub installer_log: PathBuf,
    pub old_pid: PathBuf,
    pub new_pid: PathBuf,
    pub helper_pid: PathBuf,
    pub agent_pid: PathBuf,
    pub result: PathBuf,
}

/// Resolves roots and derives safe, transaction-local marker paths. Omitted roots allocate a
/// unique temporary directory whose ownership is carried by the returned value.
pub fn resolve_paths(
    options: &UpdaterE2eOptions,
    workspace_root: &Path,
) -> Result<E2ePaths, String> {
    let (config_root, updates_root, temporary_root) = resolve_roots(options, workspace_root)?;
    if paths_overlap(&config_root, &updates_root) {
        cleanup_temporary_root(temporary_root.as_deref());
        return Err("config root and update root must be distinct and non-overlapping".to_owned());
    }
    let control_root = updates_root.join(E2E_DIR);
    let logs_root = control_root.join("logs");
    let target = options.target_version.as_deref().unwrap_or("next");
    let transaction = updates_root.join(format!("v{target}"));
    let marker = |value: Option<&Path>, default: PathBuf, label: &str| {
        let path = value
            .map(|path| absolutize(workspace_root, path))
            .unwrap_or(default);
        ensure_inside(&updates_root, &path, label)
    };
    let current = options
        .current_binary
        .as_deref()
        .map(|path| absolutize(workspace_root, path))
        .unwrap_or_else(|| updates_root.join("gmark-n"));
    let backup_default = current
        .file_name()
        .map(|name| {
            current.with_file_name(format!("{}.gmark-update-backup", name.to_string_lossy()))
        })
        .unwrap_or_else(|| control_root.join("gmark-n.gmark-update-backup"));
    let resolved = (|| {
        Ok(E2ePaths {
            temporary_root: temporary_root.clone(),
            config_root,
            updates_root: updates_root.clone(),
            control_root: control_root.clone(),
            logs_root: logs_root.clone(),
            acknowledgement: marker(
                options.acknowledgement.as_deref(),
                transaction.join("startup-ack"),
                "acknowledgement",
            )?,
            version_marker: marker(
                options.version_marker.as_deref(),
                control_root.join("version"),
                "version marker",
            )?,
            backup: options
                .backup
                .as_deref()
                .map(|path| absolutize(workspace_root, path))
                .unwrap_or(backup_default),
            installer_log: marker(
                options.installer_log.as_deref(),
                transaction.join("installer.log"),
                "installer log",
            )?,
            old_pid: marker(
                options.old_pid.as_deref(),
                control_root.join("old.pid"),
                "old PID",
            )?,
            new_pid: marker(
                options.new_pid.as_deref(),
                control_root.join("new.pid"),
                "new PID",
            )?,
            helper_pid: marker(
                options.helper_pid.as_deref(),
                control_root.join("helper.pid"),
                "helper PID",
            )?,
            agent_pid: marker(
                options.agent_pid.as_deref(),
                control_root.join("agent.pid"),
                "agent PID",
            )?,
            result: marker(
                options.result.as_deref(),
                transaction.join("result.json"),
                "result",
            )?,
        })
    })();
    if resolved.is_err() {
        cleanup_temporary_root(temporary_root.as_deref());
    }
    resolved
}

fn resolve_roots(
    options: &UpdaterE2eOptions,
    workspace_root: &Path,
) -> Result<(PathBuf, PathBuf, Option<PathBuf>), String> {
    let temporary_root = (options.config_root.is_none() || options.updates_root.is_none())
        .then(unique_temp_root)
        .transpose()?;
    let generated = temporary_root.as_deref().unwrap_or_else(|| Path::new(""));
    let config = options
        .config_root
        .as_deref()
        .map(|path| absolutize(workspace_root, path))
        .unwrap_or_else(|| generated.join("config"));
    let updates = options
        .updates_root
        .as_deref()
        .map(|path| absolutize(workspace_root, path))
        .unwrap_or_else(|| generated.join("updates"));
    if is_filesystem_root(&config) || is_filesystem_root(&updates) {
        return Err("configuration and update roots may not be filesystem roots".to_owned());
    }
    Ok((config, updates, temporary_root))
}

fn unique_temp_root() -> Result<PathBuf, String> {
    let base = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX epoch: {error}"))?
        .as_nanos();
    for attempt in 0..32_u32 {
        let path = base.join(format!(
            "gmark-updater-e2e-{}-{nanos}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!("failed to create owned temporary root: {error}"));
            }
        }
    }
    Err("failed to allocate a unique temporary root after 32 attempts".to_owned())
}

fn cleanup_temporary_root(root: Option<&Path>) {
    if let Some(root) = root {
        let _ = fs::remove_dir_all(root);
    }
}

fn absolutize(workspace_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        lexical_normalize(path)
    } else {
        lexical_normalize(&workspace_root.join(path))
    }
}

fn lexical_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn ensure_inside(root: &Path, path: &Path, label: &str) -> Result<PathBuf, String> {
    let path = lexical_normalize(path);
    if path.starts_with(root) {
        Ok(path)
    } else {
        Err(format!(
            "{label} path '{}' must be inside update root '{}'",
            path.display(),
            root.display()
        ))
    }
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn is_filesystem_root(path: &Path) -> bool {
    path.parent().is_none() || path.parent() == Some(path)
}

/// Runs the updater-e2e command from the repository-level dispatcher.
pub fn run_at(workspace_root: &Path, arguments: &[String]) -> Result<(), String> {
    let parsed = parse_args(arguments)?;
    if parsed.help {
        print_help();
        return Ok(());
    }
    let options = parsed.options;
    let paths = resolve_paths(&options, workspace_root)?;
    let mut workspace = RuntimeWorkspace::new(&options, &paths)?;
    let result = if options.fixture {
        Err(contract_error(
            &paths,
            "fixture mode is contract-only; no production pass is reported",
        ))
    } else if options.dry_run {
        println!(
            "updater-e2e dry-run: platform={}, decision={}, config-root={}, updates-root={}",
            platform_name(),
            options.decision.as_str(),
            paths.config_root.display(),
            paths.updates_root.display()
        );
        Ok(())
    } else {
        runner::execute(&options, &paths)
    };
    if result.is_ok() {
        workspace.cleanup()?;
    }
    result
}

struct RuntimeWorkspace {
    owned_root: Option<PathBuf>,
    keep_temp: bool,
}

impl RuntimeWorkspace {
    fn new(options: &UpdaterE2eOptions, paths: &E2ePaths) -> Result<Self, String> {
        fs::create_dir_all(&paths.config_root).map_err(|error| {
            format!(
                "failed to create config root '{}': {error}",
                paths.config_root.display()
            )
        })?;
        fs::create_dir_all(&paths.logs_root).map_err(|error| {
            format!(
                "failed to create update root '{}': {error}",
                paths.logs_root.display()
            )
        })?;
        Ok(Self {
            owned_root: paths.temporary_root.clone(),
            keep_temp: options.keep_temp,
        })
    }

    fn cleanup(&mut self) -> Result<(), String> {
        if self.keep_temp {
            return Ok(());
        }
        if let Some(root) = self.owned_root.take() {
            fs::remove_dir_all(&root).map_err(|error| {
                format!(
                    "failed to clean owned temporary root '{}': {error}",
                    root.display()
                )
            })?;
        }
        Ok(())
    }
}

fn platform_name() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unsupported"
    }
}

fn contract_error(paths: &E2ePaths, detail: &str) -> String {
    format!(
        "updater-e2e [preflight] {detail}; platform={}; logs={}",
        platform_name(),
        paths.logs_root.display()
    )
}

fn stage_error(
    stage: &str,
    command: &str,
    code: Option<i32>,
    paths: &E2ePaths,
    detail: &str,
) -> String {
    let exit = code.map_or_else(|| "n/a".to_owned(), |code| code.to_string());
    format!(
        "updater-e2e [{stage}] command={command}; exit-code={exit}; logs={}; {detail}",
        paths.logs_root.display()
    )
}

fn display_command(program: &Path, arguments: &[String]) -> String {
    let mut command = program.display().to_string();
    for argument in arguments {
        command.push(' ');
        command.push_str(argument);
    }
    command
}

/// The platform-specific command and marker contract printed by updater-e2e --help.
pub fn print_help() {
    println!(
        "GMark updater E2E harness\n\
         Usage: cargo run -p xtask -- updater-e2e [OPTIONS]\n\
         Required for a production run: --current-binary PATH --next-binary PATH (or --next-installer PATH),\n\
         --current-version SEMVER --target-version SEMVER --driver PATH, --signing-private-key PATH,\n\
         and --signing-public-key PATH (or --public-key-base64 VALUE).\n\
         The driver contract receives --phase unsaved-decision then trigger-update and isolated paths\n\
         through arguments and GMARK_E2E_* environment variables. It must write helper.pid,\n\
         new.pid, startup-ack (exactly '<target>\\n'), version, and installer.log for save/discard;\n\
         macOS/Linux drivers also write agent.pid, while Windows verifies Inno Setup feedback.\n\
         Platform launchers: Windows .ps1/.cmd use PowerShell/cmd; macOS and Linux use executable or .sh.\n\
         Options:\n\
         --config-root PATH --updates-root PATH --current-binary PATH --next-binary PATH\n\
         --current-installer PATH --next-installer PATH --signing-private-key PATH\n\
         --signing-public-key PATH --public-key-base64 VALUE --current-version SEMVER\n\
         --target-version SEMVER --manifest-url LOOPBACK_URL --driver PATH --helper PATH --agent PATH --apply-plan PATH\n\
         --ack-path PATH --version-path PATH --backup-path PATH --installer-log PATH\n\
         --old-pid-file PATH --new-pid-file PATH --helper-pid-file PATH --agent-pid-file PATH\n\
         --result PATH --decision cancel|save|discard --timeout 90s --dry-run --fixture --keep-temp\n\
         A missing artifact, stale marker, or absent automation driver is reported as environment not ready;\n\
         no unexecuted fixture is reported as a production pass. Failure output always includes stage,\n\
         command, exit code when available, and the preserved log directory."
    );
}
