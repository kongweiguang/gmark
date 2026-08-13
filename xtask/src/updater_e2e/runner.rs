// @author kongweiguang

use super::*;

pub(super) fn execute(options: &UpdaterE2eOptions, paths: &E2ePaths) -> Result<(), String> {
    preflight(options, paths)?;
    let mut current = spawn_current(options, paths)?;
    write_pid(&paths.old_pid, current.id())?;
    let initial_status = match current.try_wait() {
        Ok(status) => status,
        Err(error) => {
            terminate(&mut current);
            return Err(stage_error(
                "launch-N",
                "N process",
                None,
                paths,
                &error.to_string(),
            ));
        }
    };
    if let Some(status) = initial_status {
        terminate(&mut current);
        return Err(stage_error(
            "launch-N",
            "N process",
            status.code(),
            paths,
            "N exited before the automation driver could trigger an update",
        ));
    }
    let result = run_flow(options, paths, &mut current);
    if result.is_err() {
        terminate(&mut current);
    }
    result
}

fn preflight(options: &UpdaterE2eOptions, paths: &E2ePaths) -> Result<(), String> {
    let mut missing = Vec::new();
    required_file(options.current_binary.as_deref(), "N binary", &mut missing);
    if options.next_binary.is_none() && options.next_installer.is_none() {
        missing.push("N+1 binary or installer input".to_owned());
    }
    required_file(options.next_binary.as_deref(), "N+1 binary", &mut missing);
    required_file(
        options.current_installer.as_deref(),
        "N installer",
        &mut missing,
    );
    required_file(
        options.next_installer.as_deref(),
        "N+1 installer",
        &mut missing,
    );
    required_file(
        options.signing_private_key.as_deref(),
        "signing private key",
        &mut missing,
    );
    if options.signing_public_key.is_none() && options.public_key_base64.is_none() {
        missing.push("signing public key or --public-key-base64".to_owned());
    }
    required_file(
        options.signing_public_key.as_deref(),
        "signing public key",
        &mut missing,
    );
    required_file(options.driver.as_deref(), "automation driver", &mut missing);
    required_file(options.helper.as_deref(), "helper", &mut missing);
    required_file(options.agent.as_deref(), "agent", &mut missing);
    required_file(options.apply_plan.as_deref(), "apply plan", &mut missing);
    if options.current_version.as_deref().is_none() || options.target_version.as_deref().is_none() {
        missing.push("--current-version and --target-version".to_owned());
    } else if let (Some(current), Some(target)) =
        (&options.current_version, &options.target_version)
        && (!valid_version(current) || !valid_version(target) || !version_is_newer(current, target))
    {
        missing
            .push("target version must be valid SemVer and newer than current version".to_owned());
    }
    for path in [
        &paths.acknowledgement,
        &paths.version_marker,
        &paths.old_pid,
        &paths.new_pid,
        &paths.helper_pid,
        &paths.agent_pid,
        &paths.installer_log,
        &paths.result,
    ] {
        if marker_present(path) {
            missing.push(format!("stale marker must be removed: {}", path.display()));
        }
    }
    if marker_present(&paths.backup) {
        missing.push(format!(
            "backup path must be absent before run: {}",
            paths.backup.display()
        ));
    }
    if !missing.is_empty() {
        return Err(contract_error(
            paths,
            &format!("environment not ready: {}", missing.join("; ")),
        ));
    }
    Ok(())
}

fn required_file(value: Option<&Path>, label: &str, missing: &mut Vec<String>) {
    if let Some(path) = value
        && !path.is_file()
    {
        missing.push(format!("{label} '{}' does not exist", path.display()));
    }
}

fn spawn_current(options: &UpdaterE2eOptions, paths: &E2ePaths) -> Result<Child, String> {
    let binary = options
        .current_binary
        .as_deref()
        .ok_or_else(|| contract_error(paths, "launch-N: current binary was not supplied"))?;
    let log = paths.logs_root.join("n.log");
    spawn_program(
        binary,
        &[],
        &runtime_environment(options, paths, "launch-n"),
        &log,
        paths,
        "launch-N",
    )
}

fn run_flow(
    options: &UpdaterE2eOptions,
    paths: &E2ePaths,
    current: &mut Child,
) -> Result<(), String> {
    let plan = decision_plan(options.decision);
    invoke_driver(options, paths, current.id(), "unsaved-decision")?;
    if !plan.continue_install {
        if paths.helper_pid.exists() || paths.agent_pid.exists() {
            return Err(stage_error(
                "unsaved-cancel",
                "marker assertion",
                None,
                paths,
                "helper/agent marker appeared after cancel",
            ));
        }
        if current
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err(stage_error(
                "unsaved-cancel",
                "old process",
                None,
                paths,
                "N exited even though update was cancelled",
            ));
        }
        terminate(current);
        println!("updater-e2e passed: unsaved decision cancel kept the update helper stopped");
        return Ok(());
    }
    invoke_driver(options, paths, current.id(), "trigger-update")?;
    wait_for_child_exit(current, options.timeout, paths)?;
    wait_for_marker(&paths.helper_pid, options.timeout, "helper PID", paths)?;
    if expects_feedback_agent() {
        wait_for_marker(&paths.agent_pid, options.timeout, "agent PID", paths)?;
    }
    wait_for_marker(&paths.new_pid, options.timeout, "new PID", paths)?;
    wait_for_marker(
        &paths.acknowledgement,
        options.timeout,
        "startup acknowledgement",
        paths,
    )?;
    wait_for_marker(
        &paths.version_marker,
        options.timeout,
        "version marker",
        paths,
    )?;
    wait_for_marker(
        &paths.installer_log,
        options.timeout,
        "installer feedback log",
        paths,
    )?;
    assert_pid(&paths.helper_pid, "helper PID", paths)?;
    if expects_feedback_agent() {
        assert_pid(&paths.agent_pid, "agent PID", paths)?;
    }
    let new_pid = assert_pid(&paths.new_pid, "new PID", paths)?;
    let old_pid = read_pid(&paths.old_pid)
        .map_err(|error| stage_error("pid-assertion", "old PID", None, paths, &error))?;
    if new_pid == old_pid {
        return Err(stage_error(
            "pid-assertion",
            "new PID",
            None,
            paths,
            "new process reused the old PID marker",
        ));
    }
    let target = options.target_version.as_deref().unwrap_or_default();
    assert_ack_version(&paths.acknowledgement, target, paths)?;
    assert_version_marker(&paths.version_marker, target, paths)?;
    if marker_present(&paths.backup) {
        return Err(stage_error(
            "backup-cleanup",
            "backup path",
            None,
            paths,
            "update backup remains after startup acknowledgement",
        ));
    }
    if options.result.is_some() {
        assert_result(&paths.result, target, paths)?;
    }
    println!(
        "updater-e2e passed: {} -> {} (helper, platform feedback, PID, ack, version, installer log, backup cleanup)",
        options.current_version.as_deref().unwrap_or("N"),
        target
    );
    Ok(())
}

fn expects_feedback_agent() -> bool {
    // Windows intentionally exposes Inno Setup's progress UI. macOS and Linux
    // use the read-only GPUI agent when installation lasts longer than 700 ms.
    !cfg!(windows)
}

fn invoke_driver(
    options: &UpdaterE2eOptions,
    paths: &E2ePaths,
    pid: u32,
    phase: &str,
) -> Result<(), String> {
    let driver = options
        .driver
        .as_deref()
        .ok_or_else(|| contract_error(paths, "driver path is required"))?;
    let log = paths.logs_root.join(format!("driver-{phase}.log"));
    let args = [
        "--phase".to_owned(),
        phase.to_owned(),
        "--decision".to_owned(),
        options.decision.as_str().to_owned(),
        "--pid".to_owned(),
        pid.to_string(),
        "--ui-check-root".to_owned(),
        paths.ui_check_root.display().to_string(),
        "--updates-root".to_owned(),
        paths.updates_root.display().to_string(),
        "--current-binary".to_owned(),
        options
            .current_binary
            .as_deref()
            .unwrap_or(Path::new(""))
            .display()
            .to_string(),
        "--next-binary".to_owned(),
        options
            .next_binary
            .as_deref()
            .unwrap_or(Path::new(""))
            .display()
            .to_string(),
        "--current-installer".to_owned(),
        options
            .current_installer
            .as_deref()
            .unwrap_or(Path::new(""))
            .display()
            .to_string(),
        "--next-installer".to_owned(),
        options
            .next_installer
            .as_deref()
            .unwrap_or(Path::new(""))
            .display()
            .to_string(),
        "--ack".to_owned(),
        paths.acknowledgement.display().to_string(),
        "--version".to_owned(),
        options
            .target_version
            .as_deref()
            .unwrap_or_default()
            .to_owned(),
        "--old-pid".to_owned(),
        paths.old_pid.display().to_string(),
        "--new-pid".to_owned(),
        paths.new_pid.display().to_string(),
        "--helper-pid".to_owned(),
        paths.helper_pid.display().to_string(),
        "--agent-pid".to_owned(),
        paths.agent_pid.display().to_string(),
        "--installer-log".to_owned(),
        paths.installer_log.display().to_string(),
    ];
    let status = run_logged_program(
        driver,
        &args,
        &runtime_environment(options, paths, phase),
        &log,
        options.timeout,
    )
    .map_err(|error| stage_error(phase, &display_command(driver, &args), None, paths, &error))?;
    if !status.success() {
        return Err(stage_error(
            phase,
            &display_command(driver, &args),
            status.code(),
            paths,
            "automation driver exited unsuccessfully",
        ));
    }
    Ok(())
}

fn runtime_environment(
    options: &UpdaterE2eOptions,
    paths: &E2ePaths,
    phase: &str,
) -> Vec<(OsString, OsString)> {
    let mut values = vec![
        ("GMARK_E2E_PHASE".into(), phase.into()),
        ("GMARK_E2E_PLATFORM".into(), platform_name().into()),
        (
            "GMARK_E2E_DECISION".into(),
            options.decision.as_str().into(),
        ),
        (
            "GMARK_UI_CHECK_ROOT".into(),
            paths.ui_check_root.clone().into_os_string(),
        ),
        (
            "GMARK_UPDATER_E2E_UPDATE_ROOT".into(),
            paths.updates_root.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_CURRENT_BINARY".into(),
            options
                .current_binary
                .clone()
                .unwrap_or_default()
                .into_os_string(),
        ),
        (
            "GMARK_E2E_NEXT_BINARY".into(),
            options
                .next_binary
                .clone()
                .unwrap_or_default()
                .into_os_string(),
        ),
        (
            "GMARK_E2E_ACK_PATH".into(),
            paths.acknowledgement.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_VERSION_PATH".into(),
            paths.version_marker.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_BACKUP_PATH".into(),
            paths.backup.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_HELPER_PID_PATH".into(),
            paths.helper_pid.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_AGENT_PID_PATH".into(),
            paths.agent_pid.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_NEW_PID_PATH".into(),
            paths.new_pid.clone().into_os_string(),
        ),
        (
            "GMARK_E2E_INSTALLER_LOG".into(),
            paths.installer_log.clone().into_os_string(),
        ),
    ];
    if let Some(version) = &options.target_version {
        values.push(("GMARK_E2E_TARGET_VERSION".into(), version.clone().into()));
    }
    if let Some(url) = &options.manifest_url {
        values.push(("GMARK_UPDATER_E2E_MANIFEST_URL".into(), url.clone().into()));
    }
    if let Some(path) = &options.signing_private_key {
        values.push((
            "GMARK_E2E_SIGNING_PRIVATE_KEY".into(),
            path.clone().into_os_string(),
        ));
    }
    if let Some(path) = &options.signing_public_key {
        values.push((
            "GMARK_E2E_SIGNING_PUBLIC_KEY".into(),
            path.clone().into_os_string(),
        ));
    }
    if let Some(path) = &options.helper {
        values.push(("GMARK_E2E_HELPER".into(), path.clone().into_os_string()));
    }
    if let Some(path) = &options.agent {
        values.push(("GMARK_E2E_AGENT".into(), path.clone().into_os_string()));
    }
    if let Some(path) = &options.apply_plan {
        values.push(("GMARK_E2E_APPLY_PLAN".into(), path.clone().into_os_string()));
    }
    if let Some(path) = &options.current_installer {
        values.push((
            "GMARK_E2E_CURRENT_INSTALLER".into(),
            path.clone().into_os_string(),
        ));
    }
    if let Some(path) = &options.next_installer {
        values.push((
            "GMARK_E2E_NEXT_INSTALLER".into(),
            path.clone().into_os_string(),
        ));
    }
    if let Some(key) = &options.public_key_base64 {
        values.push(("GMARK_E2E_PUBLIC_KEY_BASE64".into(), key.clone().into()));
    }
    values
}

fn spawn_program(
    program: &Path,
    arguments: &[String],
    environment: &[(OsString, OsString)],
    log: &Path,
    paths: &E2ePaths,
    stage: &str,
) -> Result<Child, String> {
    let mut command = command_for_program(program, arguments);
    command.envs(environment.iter().cloned());
    let file = open_log(log).map_err(|error| {
        stage_error(
            stage,
            &display_command(program, arguments),
            None,
            paths,
            &error,
        )
    })?;
    let stderr = file
        .try_clone()
        .map_err(|error| error.to_string())
        .map_err(|error| {
            stage_error(
                stage,
                &display_command(program, arguments),
                None,
                paths,
                &error,
            )
        })?;
    command
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(stderr));
    command.spawn().map_err(|error| {
        stage_error(
            stage,
            &display_command(program, arguments),
            None,
            paths,
            &error.to_string(),
        )
    })
}

fn run_logged_program(
    program: &Path,
    arguments: &[String],
    environment: &[(OsString, OsString)],
    log: &Path,
    timeout: Duration,
) -> Result<ExitStatus, String> {
    let mut command = command_for_program(program, arguments);
    command.envs(environment.iter().cloned());
    let file = open_log(log)?;
    let stderr = file.try_clone().map_err(|error| error.to_string())?;
    command
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(stderr));
    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to run '{}': {error}", program.display()))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if std::time::Instant::now() < deadline => {
                sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "timed out after {} seconds running '{}'",
                    timeout.as_secs(),
                    program.display()
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "failed waiting for '{}': {error}",
                    program.display()
                ));
            }
        }
    }
}

fn command_for_program(program: &Path, arguments: &[String]) -> Command {
    #[cfg(windows)]
    {
        let extension = program
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if extension.eq_ignore_ascii_case("ps1") {
            let mut command = Command::new("powershell.exe");
            command
                .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(program)
                .args(arguments);
            return command;
        }
        if extension.eq_ignore_ascii_case("cmd") || extension.eq_ignore_ascii_case("bat") {
            let mut command = Command::new("cmd.exe");
            command
                .args(["/C", &program.display().to_string()])
                .args(arguments);
            return command;
        }
    }
    #[cfg(not(windows))]
    if program.extension().and_then(|value| value.to_str()) == Some("sh") {
        let mut command = Command::new("sh");
        command.arg(program).args(arguments);
        return command;
    }
    let mut command = Command::new(program);
    command.args(arguments);
    command
}

fn open_log(path: &Path) -> Result<File, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create log directory '{}': {error}",
                parent.display()
            )
        })?;
    }
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("failed to open log '{}': {error}", path.display()))
}

fn write_pid(path: &Path, pid: u32) -> Result<(), String> {
    let mut file = File::create(path)
        .map_err(|error| format!("failed to write PID marker '{}': {error}", path.display()))?;
    writeln!(file, "{pid}")
        .map_err(|error| format!("failed to flush PID marker '{}': {error}", path.display()))
}

fn wait_for_child_exit(
    child: &mut Child,
    timeout: Duration,
    paths: &E2ePaths,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() || status.code().is_some() => return Ok(()),
            Ok(Some(status)) => {
                return Err(stage_error(
                    "old-pid-exit",
                    "N process",
                    status.code(),
                    paths,
                    "N exited without a portable exit code",
                ));
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                sleep(Duration::from_millis(25));
                continue;
            }
            Ok(None) => {
                return Err(stage_error(
                    "old-pid-exit",
                    "N process",
                    None,
                    paths,
                    "timed out waiting for old PID to exit",
                ));
            }
            Err(error) => {
                return Err(stage_error(
                    "old-pid-exit",
                    "N process",
                    None,
                    paths,
                    &error.to_string(),
                ));
            }
        }
    }
}

fn wait_for_marker(
    path: &Path,
    timeout: Duration,
    label: &str,
    paths: &E2ePaths,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + timeout;
    while !marker_present(path) && std::time::Instant::now() < deadline {
        sleep(Duration::from_millis(25));
    }
    if marker_is_regular(path) {
        Ok(())
    } else {
        Err(stage_error(
            "marker-wait",
            &format!("{label} '{}'", path.display()),
            None,
            paths,
            "timed out or marker is not a regular file",
        ))
    }
}

fn assert_pid(path: &Path, label: &str, paths: &E2ePaths) -> Result<u32, String> {
    read_pid(path).map_err(|error| {
        stage_error(
            "pid-assertion",
            &format!("{label} '{}'", path.display()),
            None,
            paths,
            &error,
        )
    })
}

fn read_pid(path: &Path) -> Result<u32, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read PID marker '{}': {error}", path.display()))?;
    let value = text
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("PID marker '{}' is not an integer", path.display()))?;
    if value == 0 {
        return Err(format!("PID marker '{}' must be positive", path.display()));
    }
    u32::try_from(value).map_err(|_| format!("PID marker '{}' exceeds u32", path.display()))
}

fn marker_present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn marker_is_regular(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file())
        .unwrap_or(false)
}

/// Checks the exact version-text marker used by the updater startup handshake.
pub fn parse_ack_version(bytes: &[u8]) -> Result<String, String> {
    if bytes.is_empty() || !bytes.ends_with(b"\n") {
        return Err(
            "startup acknowledgement must contain one newline-terminated version".to_owned(),
        );
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "startup acknowledgement is not UTF-8".to_owned())?;
    let version = text.strip_suffix('\n').unwrap_or_default();
    if version.is_empty() || version.contains('\n') || !valid_version(version) {
        return Err("startup acknowledgement contains an invalid version".to_owned());
    }
    Ok(version.to_owned())
}

fn assert_ack_version(path: &Path, expected: &str, paths: &E2ePaths) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| {
        stage_error(
            "ack-assertion",
            &format!("ack '{}'", path.display()),
            None,
            paths,
            &error.to_string(),
        )
    })?;
    let actual = parse_ack_version(&bytes).map_err(|error| {
        stage_error(
            "ack-assertion",
            &format!("ack '{}'", path.display()),
            None,
            paths,
            &error,
        )
    })?;
    if actual != expected {
        return Err(stage_error(
            "ack-assertion",
            &format!("ack '{}'", path.display()),
            None,
            paths,
            &format!("expected version {expected}, got {actual}"),
        ));
    }
    Ok(())
}

fn assert_version_marker(path: &Path, expected: &str, paths: &E2ePaths) -> Result<(), String> {
    let actual = fs::read_to_string(path).map_err(|error| {
        stage_error(
            "version-assertion",
            &format!("version '{}'", path.display()),
            None,
            paths,
            &error.to_string(),
        )
    })?;
    if actual.trim() != expected {
        return Err(stage_error(
            "version-assertion",
            &format!("version '{}'", path.display()),
            None,
            paths,
            &format!("expected {expected}, got {}", actual.trim()),
        ));
    }
    Ok(())
}

fn assert_result(path: &Path, expected: &str, paths: &E2ePaths) -> Result<(), String> {
    let result = fs::read_to_string(path).map_err(|error| {
        stage_error(
            "result-assertion",
            &format!("result '{}'", path.display()),
            None,
            paths,
            &error.to_string(),
        )
    })?;
    if !result.contains("succeeded") || !result.contains(expected) {
        return Err(stage_error(
            "result-assertion",
            &format!("result '{}'", path.display()),
            None,
            paths,
            "result did not contain succeeded status and target version",
        ));
    }
    Ok(())
}

fn terminate(child: &mut Child) {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
