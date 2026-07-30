// @author kongweiguang

//! macOS command-line tool installation actions.

use super::*;

#[cfg(target_os = "macos")]
pub(crate) fn install_cli_tool(cx: &mut App) {
    use std::process::Command;

    let bin_link = "/usr/local/bin/gmark";
    let strings = cx.global::<I18nManager>().strings();

    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            show_install_cli_error(cx, &format!("Failed to get executable path: {err}"));
            return;
        }
    };

    // Only allow from a portable .app bundle (e.g. drag-installed to /Applications)
    if !current_exe
        .to_string_lossy()
        .contains(".app/Contents/MacOS/")
    {
        show_install_cli_error(
            cx,
            "Command-line tool installation requires running from an .app bundle.\n\n\
             If the app was installed via the `.pkg` installer,\n\
             the CLI command is configured automatically.",
        );
        return;
    }

    let exe_path = applescript_string_literal(&current_exe.to_string_lossy());
    let link_path = applescript_string_literal(bin_link);
    let script = format!(
        r#"set exePath to {exe_path}
set linkPath to {link_path}
do shell script "rm -f " & quoted form of linkPath & linefeed & "ln -s " & quoted form of exePath & space & quoted form of linkPath with administrator privileges"#
    );

    match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(output) => {
            if output.status.success() {
                let title = "CLI Command Installed";
                let detail = format!(
                    "Successfully installed! You can now use 'gmark' from the terminal:\n\n\
                     \x1b[1mgmark README.md\x1b[0m\n\
                     \x1b[1mgmark file1.md file2.md\x1b[0m\n\n\
                     Location: {bin_link}\n\n\
                     Note: If you move or delete gmark.app,\n\
                     the 'gmark' command will stop working\n\
                     automatically (no cleanup needed)."
                );
                if let Some(window) = cx.active_window() {
                    let ok = strings.info_dialog_ok.clone();
                    let _ = window.update(cx, |_view, window, cx| {
                        let _ = window.prompt(
                            PromptLevel::Info,
                            &title,
                            Some(&detail),
                            &[ok.as_str()],
                            cx,
                        );
                    });
                }
            } else {
                // User pressed Cancel on the admin password dialog
                // or the link creation failed for another reason.
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let detail = if stderr.contains("User canceled") || stderr.contains("(-128)") {
                    "Installation cancelled.".to_string()
                } else {
                    format!("Installation failed: {stderr}")
                };
                show_install_cli_error(cx, &detail);
            }
        }
        Err(err) => {
            show_install_cli_error(cx, &format!("Failed to run installer: {err}"));
        }
    }
    // Refresh menus so the label changes between Install -> Uninstall.
    install_menus(cx);
}

#[cfg(target_os = "macos")]
pub(crate) fn uninstall_cli_tool(cx: &mut App) {
    use std::process::Command;

    let bin_link = "/usr/local/bin/gmark";
    let strings = cx.global::<I18nManager>().strings();

    if !is_cli_symlink_current_app() {
        show_install_cli_error(cx, "CLI command is not installed for this app.");
        return;
    }

    let link_path = applescript_string_literal(bin_link);
    let script = format!(
        r#"set linkPath to {link_path}
do shell script "rm -f " & quoted form of linkPath with administrator privileges"#
    );

    match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(output) => {
            if output.status.success() {
                let title = "CLI Command Uninstalled";
                let detail = "CLI command has been removed successfully.".to_string();
                if let Some(window) = cx.active_window() {
                    let ok = strings.info_dialog_ok.clone();
                    let _ = window.update(cx, |_view, window, cx| {
                        let _ = window.prompt(
                            PromptLevel::Info,
                            &title,
                            Some(&detail),
                            &[ok.as_str()],
                            cx,
                        );
                    });
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let detail = if stderr.contains("User canceled") || stderr.contains("(-128)") {
                    "Uninstall cancelled.".to_string()
                } else {
                    format!("Uninstall failed: {stderr}")
                };
                show_install_cli_error(cx, &detail);
            }
        }
        Err(err) => {
            show_install_cli_error(cx, &format!("Failed to run uninstaller: {err}"));
        }
    }
    // Refresh menus so the label changes between Install -> Uninstall.
    install_menus(cx);
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn install_cli_tool(cx: &mut App) {
    show_install_cli_error(
        cx,
        "Command-line tool installation is only available on macOS.",
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn uninstall_cli_tool(cx: &mut App) {
    show_install_cli_error(
        cx,
        "Command-line tool uninstallation is only available on macOS.",
    );
}

fn show_install_cli_error(cx: &mut App, detail: &str) {
    let strings = cx.global::<I18nManager>().strings();
    let title = "Install Command-Line Tool Failed";

    if let Some(window) = cx.active_window() {
        let ok = strings.info_dialog_ok.clone();
        let _ = window.update(cx, |_view, window, cx| {
            let _ = window.prompt(
                PromptLevel::Critical,
                title,
                Some(detail),
                &[ok.as_str()],
                cx,
            );
        });
    } else {
        eprintln!("{title}: {detail}");
    }
}
