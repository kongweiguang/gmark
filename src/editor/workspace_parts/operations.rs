// @author kongweiguang

use super::*;
use crate::editor::DocumentMenuFormat;
use crate::editor::render::dialog_body;

fn format_sidebar_byte_count(bytes: u64) -> String {
    const KIB: u64 = 1_024;
    const MIB: u64 = KIB * 1_024;
    const GIB: u64 = MIB * 1_024;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[path = "operations/dialogs.rs"]
mod dialogs;
#[path = "operations/document_sidebar.rs"]
mod document_sidebar;
#[path = "operations/move.rs"]
mod move_operations;
#[path = "operations/panel.rs"]
mod panel;
#[path = "operations/quick_open.rs"]
mod quick_open;
