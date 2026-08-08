// @author kongweiguang

use std::{
    env,
    process::Command,
    str,
    time::{SystemTime, UNIX_EPOCH},
};

const BOARD_EVIDENCE_FEATURE_ENV: &str = "CARGO_FEATURE_BOARD_EVIDENCE";
const BUILD_GIT_SHA_ENV: &str = "GMARK_BUILD_GIT_SHA";
const BUILD_WORKSPACE_DIRTY_ENV: &str = "GMARK_BUILD_WORKSPACE_DIRTY";
const BUILD_DATE_ENV: &str = "GMARK_BUILD_DATE_UTC";

fn main() {
    println!("cargo:rerun-if-changed=resources/windows/gmark.rc");
    println!("cargo:rerun-if-changed=assets/icon/gmark.ico");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=.git/packed-refs");
    println!("cargo:rerun-if-changed=.git/refs");

    if env::var_os(BOARD_EVIDENCE_FEATURE_ENV).is_some() {
        emit_board_evidence_build_metadata().unwrap_or_else(|error| {
            panic!(
                "board-evidence build metadata is unavailable; refusing a traceability gap: {error}"
            )
        });
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-arg-bin=gmark=/STACK:8388608");
        embed_resource::compile("resources/windows/gmark.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to compile gmark Windows resources");
    }
}

fn emit_board_evidence_build_metadata() -> Result<(), String> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| "CARGO_MANIFEST_DIR is not set".to_owned())?;
    let git_sha = git_output(&manifest_dir, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    validate_git_sha(&git_sha)?;
    let workspace_dirty = !git_output(
        &manifest_dir,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    )?
    .is_empty();
    let build_date = utc_date()?;

    println!("cargo:rustc-env={BUILD_GIT_SHA_ENV}={git_sha}");
    println!(
        "cargo:rustc-env={BUILD_WORKSPACE_DIRTY_ENV}={}",
        if workspace_dirty { "true" } else { "false" }
    );
    println!("cargo:rustc-env={BUILD_DATE_ENV}={build_date}");
    Ok(())
}

fn git_output(manifest_dir: &std::ffi::OsStr, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(manifest_dir)
        .output()
        .map_err(|error| format!("failed to execute git {:?}: {error}", arguments))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "git {:?} exited with {}; stderr: {}",
            arguments,
            output.status,
            stderr.trim()
        ));
    }
    let value = str::from_utf8(&output.stdout)
        .map_err(|error| format!("git {:?} returned non-UTF-8 output: {error}", arguments))?
        .trim()
        .to_owned();
    Ok(value)
}

fn validate_git_sha(value: &str) -> Result<(), String> {
    if value.len() != 40 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "git rev-parse returned an invalid commit SHA {value:?}; expected 40 lowercase hexadecimal characters"
        ));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(format!(
            "git rev-parse returned an uppercase commit SHA {value:?}; expected lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

fn utc_date() -> Result<String, String> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock precedes UNIX epoch: {error}"))?
        .as_secs();
    let days = i64::try_from(seconds / 86_400)
        .map_err(|_| "UTC day count does not fit a signed 64-bit integer".to_owned())?;

    // Howard Hinnant's civil_from_days conversion, kept local so build.rs has no date-time
    // dependency. The output is UTC and deliberately date-only for a stable review record.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let day_of_year = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = month_part + if month_part < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    Ok(format!("{year:04}-{month:02}-{day:02}"))
}
