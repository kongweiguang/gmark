// @author kongweiguang

//! 外部格式化器的进程边界；领域 crate 不执行 Shell 或访问工作区配置。

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gmark_paged_document::SearchCancellation;

use super::formatting::{ExternalFormatterSpec, FormatError};

const MAX_STDERR_BYTES: usize = 1024 * 1024;

pub(crate) fn run_shell_formatter(
    spec: &ExternalFormatterSpec,
    input: &[u8],
    cancellation: &SearchCancellation,
) -> Result<String, FormatError> {
    if spec.selection.is_some() && !spec.supports_range {
        return Err(FormatError::MissingFormatter(
            "当前格式化器不支持选区格式化".to_owned(),
        ));
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", &spec.command]);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    };
    #[cfg(not(target_os = "windows"))]
    let mut command = {
        use std::os::unix::process::CommandExt as _;
        let mut command = Command::new("/bin/sh");
        command.args(["-lc", &spec.command]);
        // 独立进程组让取消/超时能够终止 Shell 及其所有后代，而不波及 GMark。
        command.process_group(0);
        command
    };
    command
        .current_dir(&spec.cwd)
        .env("GMARK_FILE", &spec.file)
        .env("GMARK_LANGUAGE", spec.language.canonical_name())
        .env(
            "GMARK_RANGE_START",
            spec.selection
                .as_ref()
                .map_or(0, |range| range.start)
                .to_string(),
        )
        .env(
            "GMARK_RANGE_END",
            spec.selection
                .as_ref()
                .map_or(0, |range| range.end)
                .to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| FormatError::External(format!("无法启动格式化器：{error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| FormatError::External("格式化器 stdin 不可用".to_owned()))?;
    let input = input.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| FormatError::External("格式化器 stdout 不可用".to_owned()))?;
    let output_limit = spec.max_output_bytes;
    let output_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .by_ref()
            .take(output_limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| FormatError::External("格式化器 stderr 不可用".to_owned()))?;
    let error_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr
            .by_ref()
            .take(MAX_STDERR_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        if cancellation.is_cancelled() {
            terminate_formatter(&mut child);
            break Err(FormatError::Cancelled);
        }
        if started.elapsed() >= spec.timeout {
            terminate_formatter(&mut child);
            break Err(FormatError::TimedOut);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                terminate_formatter(&mut child);
                break Err(FormatError::External(format!("等待格式化器失败：{error}")));
            }
        }
    };
    // 无论成功、失败、取消或超时，都等待三个管道线程退出，保证没有后台线程继续
    // 持有文档输入或子进程句柄。
    let _ = writer.join();
    let output = output_reader
        .join()
        .map_err(|_| FormatError::External("读取格式化输出失败".to_owned()))?
        .map_err(|error| FormatError::External(error.to_string()))?;
    let stderr = error_reader
        .join()
        .map_err(|_| FormatError::External("读取格式化错误失败".to_owned()))?
        .map_err(|error| FormatError::External(error.to_string()))?;
    let status = status?;
    if output.len() > spec.max_output_bytes {
        return Err(FormatError::OutputTooLarge);
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr);
        return Err(FormatError::External(format!(
            "格式化器退出状态 {status}：{}",
            detail.trim()
        )));
    }
    String::from_utf8(output).map_err(|_| FormatError::InvalidUtf8)
}

fn terminate_formatter(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = Command::new("kill")
            .args(["-TERM", "--", &format!("-{}", child.id())])
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}
