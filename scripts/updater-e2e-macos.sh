#!/usr/bin/env bash
# @author kongweiguang
# This driver uses the real macOS accessibility surface so updater E2E cannot
# pass by manufacturing the helper, agent, PID, or result protocol markers.
set -euo pipefail

fail() {
    echo "[updater-e2e-macos] error: $*" >&2
    exit 1
}

usage() {
    cat <<'USAGE'
Usage: updater-e2e-macos.sh \
  --phase unsaved-decision|trigger-update \
  --decision cancel|save|discard \
  --pid PID \
  --ui-check-root PATH \
  --updates-root PATH \
  --current-binary PATH \
  --version VERSION

The script drives the already-running GMark process supplied by xtask.  It
does not launch a replacement app and it does not write updater protocol
markers; the running app, helper, and feedback agent must produce those.
USAGE
}

PHASE=
DECISION=
PID=
UI_CHECK_ROOT=
UPDATES_ROOT=
CURRENT_BINARY=
TARGET_VERSION=

while (($# > 0)); do
    case "$1" in
        --phase)
            (($# >= 2)) || fail "--phase requires a value"
            PHASE=$2
            shift 2
            ;;
        --decision)
            (($# >= 2)) || fail "--decision requires a value"
            DECISION=$2
            shift 2
            ;;
        --pid)
            (($# >= 2)) || fail "--pid requires a value"
            PID=$2
            shift 2
            ;;
        --ui-check-root)
            (($# >= 2)) || fail "--ui-check-root requires a value"
            UI_CHECK_ROOT=$2
            shift 2
            ;;
        --updates-root)
            (($# >= 2)) || fail "--updates-root requires a value"
            UPDATES_ROOT=$2
            shift 2
            ;;
        --current-binary)
            (($# >= 2)) || fail "--current-binary requires a value"
            CURRENT_BINARY=$2
            shift 2
            ;;
        --version)
            (($# >= 2)) || fail "--version requires a value"
            TARGET_VERSION=$2
            shift 2
            ;;
        --next-binary|--current-installer|--next-installer|--ack|--lifetime-lock|--helper-log|--old-pid|--new-pid|--helper-pid|--agent-pid|--installer-log|--result)
            # xtask passes these paths so one driver contract is shared across
            # platforms; the app and runner, not this UI layer, own their writes.
            (($# >= 2)) || fail "$1 requires a value"
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            fail "unknown argument: $1"
            ;;
    esac
done

[[ "$PHASE" == "unsaved-decision" || "$PHASE" == "trigger-update" ]] || \
    fail "--phase must be unsaved-decision or trigger-update"
[[ "$DECISION" == "cancel" || "$DECISION" == "save" || "$DECISION" == "discard" ]] || \
    fail "--decision must be cancel, save, or discard"
[[ -n "$PID" && "$PID" != *[!0-9]* ]] || fail "--pid must be a numeric process id"
[[ -n "$UI_CHECK_ROOT" ]] || fail "--ui-check-root must not be empty"
[[ -n "$UPDATES_ROOT" ]] || fail "--updates-root must not be empty"
[[ -n "$CURRENT_BINARY" ]] || fail "--current-binary must not be empty"
[[ -n "$TARGET_VERSION" ]] || fail "--version must not be empty"

# A non-Aqua runner cannot expose the app's real menu and dialog tree, so
# rejecting it early is safer than reporting a misleading updater result.
[[ "$(uname -s)" == "Darwin" ]] || \
    fail "macOS updater E2E requires Darwin with an active Aqua GUI session"
command -v osascript >/dev/null 2>&1 || fail "osascript is required"

# The runner starts this exact binary; checking its bundle prevents accidentally
# driving a development executable that has no published helper/agent layout.
case "$CURRENT_BINARY" in
    */Contents/MacOS/gmark)
        APP_DIR=${CURRENT_BINARY%/Contents/MacOS/gmark}
        ;;
    *)
        fail "--current-binary must point to <name>.app/Contents/MacOS/gmark"
        ;;
esac
case "$APP_DIR" in
    *.app) ;;
    *) fail "--current-binary is not inside an App Bundle" ;;
esac
[[ -x "$CURRENT_BINARY" ]] || fail "current app executable is not executable: $CURRENT_BINARY"
[[ -f "$APP_DIR/Contents/Info.plist" ]] || fail "missing App Bundle Info.plist: $APP_DIR"
[[ -x "$APP_DIR/Contents/Helpers/gmark-update-helper" ]] || \
    fail "missing executable update helper in current App Bundle"
[[ -x "$APP_DIR/Contents/Helpers/gmark-update-agent" ]] || \
    fail "missing executable feedback agent in current App Bundle"

# System Events is the only supported bridge here because GPUI's custom
# controls do not guarantee stable native button roles on every macOS runner.
if ! osascript -e 'tell application "System Events" to return (count of processes)' >/dev/null 2>&1; then
    fail "macOS Aqua GUI or Accessibility permission is unavailable for System Events"
fi

osascript - "$PHASE" "$DECISION" "$PID" "$UI_CHECK_ROOT" "$TARGET_VERSION" <<'APPLESCRIPT'
-- The app process is identified by PID so a second GMark window/process cannot
-- accidentally satisfy the test after an updater restart.
on find_process(pid_value)
    tell application "System Events"
        repeat with candidate in every process
            try
                if (unix id of candidate as integer) is pid_value then
                    return contents of candidate
                end if
            end try
        end repeat
    end tell
    return missing value
end find_process

-- Reading the PID inside System Events avoids relying on a process reference's
-- implicit application context after a nested tell block returns.
on process_id(target_process)
    tell application "System Events" to return (unix id of target_process as integer)
end process_id

-- The runner owns process startup; waiting here makes a transient GUI launch
-- delay explicit instead of turning it into a false "button missing" result.
on wait_for_process(pid_value, timeout_seconds)
    set deadline to (current date) + timeout_seconds
    repeat while (current date) < deadline
        set target_process to my find_process(pid_value)
        if target_process is not missing value then return target_process
        delay 0.2
    end repeat
    error "the xtask-started GMark process was not visible to System Events"
end wait_for_process

-- Bringing the exact process frontmost is necessary on shared GitHub-hosted
-- runners where a login-window or another test application may own focus.
on focus_process(target_process)
    tell application "System Events"
        tell target_process
            set frontmost to true
            if (count of windows) is 0 then error "GMark has no GUI window"
        end tell
    end tell
end focus_process

-- A short name scan is more stable than hard-coded pixels when a runner uses a
-- different scale factor or when the app is running on Intel versus Apple Silicon.
on has_any_name(target_process, candidate_names)
    tell application "System Events"
        tell target_process
            try
                repeat with element_ref in entire contents of window 1
                    try
                        set element_name to (name of element_ref) as text
                        if element_name is in candidate_names then return true
                    end try
                    try
                        set element_description to (description of element_ref) as text
                        if element_description is in candidate_names then return true
                    end try
                end repeat
            on error
                return false
            end try
        end tell
    end tell
    return false
end has_any_name

-- Clicking an accessible element keeps this driver independent of window size;
-- attempting the element itself also handles GPUI controls exposed as groups.
on click_named(target_process, candidate_names, timeout_seconds)
    set deadline to (current date) + timeout_seconds
    repeat while (current date) < deadline
        tell application "System Events"
            tell target_process
                try
                    repeat with element_ref in entire contents of window 1
                        try
                            set element_name to (name of element_ref) as text
                            if element_name is in candidate_names then
                                try
                                    click element_ref
                                    return true
                                end try
                            end if
                        end try
                        try
                            set element_description to (description of element_ref) as text
                            if element_description is in candidate_names then
                                try
                                    click element_ref
                                    return true
                                end try
                            end if
                        end try
                    end repeat
                end try
            end tell
        end tell
        delay 0.2
    end repeat
    return false
end click_named

-- Coordinates are deliberately a last resort for GPUI builds that expose no
-- AXButton role; the app's update panel and close dialog have fixed internal
-- geometry, while name-based clicks remain the normal path.
on window_bounds(target_process)
    tell application "System Events"
        tell target_process
            set window_position to position of window 1
            set window_size to size of window 1
        end tell
    end tell
    return {item 1 of window_position, item 2 of window_position, item 1 of window_size, item 2 of window_size}
end window_bounds

on click_at(target_process, x_value, y_value)
    tell application "System Events"
        tell target_process
            click at {x_value, y_value}
        end tell
    end tell
end click_at

-- The first phase deliberately cancels the close. This proves the ordinary
-- confirmation is real while keeping N alive for the update phase that follows.
on exercise_unsaved_confirmation(target_process, target_version)
    my focus_process(target_process)
    tell application "System Events"
        tell target_process
            key code 53
        end tell
    end tell
    delay 0.3
    set bounds to my window_bounds(target_process)
    set center_x to (item 1 of bounds) + ((item 3 of bounds) / 2)
    set center_y to (item 2 of bounds) + ((item 4 of bounds) / 2)
    my click_at(target_process, center_x, center_y)
    tell application "System Events"
        tell target_process
            keystroke ("gmark updater e2e " & target_version)
            keystroke "q" using {command down}
        end tell
    end tell
    set close_names to {"Keep Editing", "继续编辑"}
    set all_close_names to {"Keep Editing", "继续编辑", "Discard and Close", "放弃并关闭", "丢弃并关闭", "Save and Close", "保存并关闭"}
    if not my has_any_name(target_process, all_close_names) then
        error "ordinary unsaved-close confirmation did not appear after Command-Q"
    end if
    if not my click_named(target_process, close_names, 5) then
        -- Dialog geometry is stable because the layout uses a centered 520px panel.
        set bounds to my window_bounds(target_process)
        my click_at(target_process, center_x - 120, center_y + 64)
    end if
    delay 0.5
    if my find_process(my process_id(target_process)) is missing value then
        error "Keep Editing did not leave the original GMark process alive"
    end if
end exercise_unsaved_confirmation

-- Iterating menus by their item text avoids relying on localized menu indexes;
-- opening a menu is harmless, while clicking the exact update item is required.
on click_update_menu(target_process)
    set update_names to {"Check for Updates", "检查更新"}
    tell application "System Events"
        tell target_process
            repeat with menu_bar_item_ref in menu bar items of menu bar 1
                try
                    click menu_bar_item_ref
                    delay 0.2
                    repeat with menu_item_ref in menu items of menu 1 of menu_bar_item_ref
                        try
                            set item_name to (name of menu_item_ref) as text
                            if item_name is in update_names then
                                click menu_item_ref
                                return true
                            end if
                        end try
                    end repeat
                    key code 53
                end try
            end repeat
        end tell
    end tell
    return false
end click_update_menu

-- The primary update action is right aligned in the fixed-width update panel;
-- trying nearby vertical positions accommodates status-bar scale differences.
on click_update_primary(target_process)
    set bounds to my window_bounds(target_process)
    set right_edge to (item 1 of bounds) + (item 3 of bounds)
    set bottom_edge to (item 2 of bounds) + (item 4 of bounds)
    repeat with y_offset in {52, 66, 80}
        my click_at(target_process, right_edge - 88, bottom_edge - (y_offset as integer))
        delay 0.5
        if my has_any_name(target_process, {"Restart and Install", "重启并安装"}) then return true
    end repeat
    return false
end click_update_primary

-- The same close dialog is used by install-and-restart; selecting the requested
-- action here lets the runner verify the actual helper handoff for save/discard.
on choose_close_action(target_process, decision)
    if decision is "save" then
        set action_names to {"Save and Close", "保存并关闭"}
        set x_offset to 180
    else if decision is "discard" then
        set action_names to {"Discard and Close", "放弃并关闭", "丢弃并关闭"}
        set x_offset to 55
    else
        error "trigger-update cannot use cancel; the first phase owns the cancel path"
    end if
    if my click_named(target_process, action_names, 5) then return
    set bounds to my window_bounds(target_process)
    set center_x to (item 1 of bounds) + ((item 3 of bounds) / 2)
    set center_y to (item 2 of bounds) + ((item 4 of bounds) / 2)
    my click_at(target_process, center_x + x_offset, center_y + 64)
end choose_close_action

-- Save-and-close opens a native NSSavePanel for the initial untitled document;
-- selecting the supplied UI sandbox keeps that side effect out of the source tree.
on complete_save_sheet(target_process, ui_check_root)
    set deadline to (current date) + 30
    repeat while (current date) < deadline
        tell application "System Events"
            tell target_process
                try
                    if (count of sheets of window 1) > 0 then
                        keystroke "g" using {command down, shift down}
                        delay 0.5
                        keystroke ui_check_root
                        key code 36
                        delay 0.5
                        try
                            set value of text field 1 of sheet 1 of window 1 to "gmark-updater-e2e.md"
                        end try
                        repeat with button_ref in buttons of sheet 1 of window 1
                            try
                                set button_name to (name of button_ref) as text
                                if button_name is in {"Save", "保存"} then
                                    click button_ref
                                    return true
                                end if
                            end try
                        end repeat
                    end if
                end try
            end tell
        end tell
        delay 0.3
    end repeat
    error "Save and Close opened no completable native save panel"
end complete_save_sheet

on run argv
    if (count of argv) < 5 then error "driver arguments are incomplete"
    set phase_name to item 1 of argv
    set decision_name to item 2 of argv
    set pid_text to item 3 of argv
    set ui_check_root to item 4 of argv
    set target_version to item 5 of argv
    try
        set pid_value to pid_text as integer
    on error
        error "driver PID is not numeric"
    end try
    set target_process to my wait_for_process(pid_value, 20)
    my focus_process(target_process)

    if phase_name is "unsaved-decision" then
        my exercise_unsaved_confirmation(target_process, target_version)
        return
    end if

    if phase_name is not "trigger-update" then error "unsupported updater E2E phase"
    if decision_name is "cancel" then error "cancel must finish in unsaved-decision"
    if not my click_update_menu(target_process) then
        error "Help > Check for Updates was not exposed by the real app menu"
    end if
    if not my wait_for_names(target_process, {"Download Update", "下载更新"}, 30) then
        error "the real update popover did not expose Download Update"
    end if
    if not my click_named(target_process, {"Download Update", "下载更新"}, 5) then
        my click_update_primary(target_process)
    end if
    if not my wait_for_names(target_process, {"Restart and Install", "重启并安装"}, 180) then
        error "the real update popover did not reach the ready-to-install state"
    end if
    if not my click_named(target_process, {"Restart and Install", "重启并安装"}, 5) then
        if not my click_update_primary(target_process) then
            error "Restart and Install could not be activated in the real update popover"
        end if
    end if
    if not my wait_for_names(target_process, {"Keep Editing", "继续编辑", "Discard and Close", "放弃并关闭", "丢弃并关闭", "Save and Close", "保存并关闭"}, 20) then
        error "install-and-restart did not show the ordinary unsaved-close confirmation"
    end if
    my choose_close_action(target_process, decision_name)
    if decision_name is "save" then my complete_save_sheet(target_process, ui_check_root)
end run

-- Kept below the main flow so polling remains centralized and every state
-- transition has one timeout that produces actionable CI diagnostics.
on wait_for_names(target_process, candidate_names, timeout_seconds)
    set deadline to (current date) + timeout_seconds
    repeat while (current date) < deadline
        if my has_any_name(target_process, candidate_names) then return true
        delay 0.2
    end repeat
    return false
end wait_for_names
APPLESCRIPT
echo "[updater-e2e-macos] completed real GUI phase: $PHASE ($DECISION)"
