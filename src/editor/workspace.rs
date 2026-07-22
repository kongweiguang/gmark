// @author kongweiguang

//! Lightweight workspace panel state, file-tree scanning, and outline parsing.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context as _, Result};
use gpui::*;

use super::{
    Block, BlockKind, BlockRecord, ContextMenuState, DocumentKind, Editor, UndoSelectionSnapshot,
    ViewMode,
    render::{
        DialogButtonKind, DialogTitleIcon, dialog_actions, dialog_button, dialog_content,
        dialog_panel, dialog_title_with_icon, modal_overlay,
    },
    workspace_file_ops,
};
use crate::components::BlockEvent;
use crate::config::WorkspaceSidebarPosition;
use crate::i18n::I18nStrings;
use crate::theme::Theme;
use crate::window_chrome::middle_ellipsis;

const FOLDER_ICON: &str = "icon/workspace/folder.svg";
const MARKDOWN_ICON: &str = "icon/workspace/markdown.svg";
const FILE_ICON: &str = "icon/ui/file.svg";
const FILES_TAB_ICON: &str = "icon/ui/files.svg";
const OUTLINE_TAB_ICON: &str = "icon/ui/outline.svg";
const SEARCH_TAB_ICON: &str = "icon/ui/search.svg";
const CHEVRON_RIGHT_ICON: &str = "icon/ui/chevron-right.svg";
const CHEVRON_DOWN_ICON: &str = "icon/ui/chevron-down.svg";
const CLOSE_ICON: &str = "icon/ui/close.svg";
const REFRESH_ICON: &str = "icon/ui/refresh.svg";
const CHECK_ICON: &str = "icon/ui/check.svg";
const WARNING_ICON: &str = "icon/ui/triangle-alert.svg";
const WORKSPACE_PANEL_TARGET_RATIO: f32 = 0.15;
const WORKSPACE_PANEL_AUTO_MIN_WIDTH: f32 = 248.0;
const WORKSPACE_PANEL_MIN_WIDTH: f32 = 200.0;
const WORKSPACE_PANEL_MAX_WIDTH: f32 = 360.0;
pub(super) const WORKSPACE_COMPACT_BREAKPOINT: f32 = 900.0;
const WORKSPACE_COMPACT_OVERLAY_WIDTH: f32 = 280.0;
const WORKSPACE_RESIZE_HIT_WIDTH: f32 = 7.0;
const WORKSPACE_RESIZE_KEYBOARD_STEP: f32 = 4.0;
const WORKSPACE_RESIZE_KEYBOARD_LARGE_STEP: f32 = 16.0;
const WORKSPACE_NODE_HEIGHT: f32 = 26.0;
const WORKSPACE_NODE_INDENT: f32 = 14.0;
const SEARCH_DEBOUNCE: Duration = Duration::from_millis(250);
const QUICK_OPEN_DEBOUNCE: Duration = Duration::from_millis(30);
const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
const QUICK_OPEN_MAX_RESULTS: usize = 100;
const SEARCH_MAX_RESULTS: usize = 500;
const SEARCH_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

fn workspace_status_row(
    id: &'static str,
    icon_selector: &'static str,
    icon: &'static str,
    message: String,
    color: Hsla,
    font_size: f32,
) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .min_h(px(22.0))
        .w_full()
        .px(px(8.0))
        .py(px(2.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .text_size(px(font_size))
        .text_color(color)
        .child(
            div()
                .id(icon_selector)
                .debug_selector(move || icon_selector.to_owned())
                .size(px(18.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    svg()
                        .path(icon)
                        .size(px(14.0))
                        .text_color(color)
                        .debug_selector(move || format!("{icon_selector}-svg")),
                ),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .line_height(px(font_size * 1.35))
                .truncate()
                .child(message),
        )
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum WorkspaceTab {
    #[default]
    Files,
    Outline,
    Search,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct WorkspaceSearchOptions {
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspaceSearchMatch {
    path: PathBuf,
    relative_path: String,
    line: usize,
    column: usize,
    preview: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct QuickOpenMatch {
    path: PathBuf,
    relative_path: String,
    score: i64,
}

struct QuickOpenState {
    input: Entity<Block>,
    results: Vec<QuickOpenMatch>,
    selected: usize,
    running: bool,
    generation: u64,
    task: Option<Task<()>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceOperationKind {
    Rename,
    Move,
    NewFile,
    NewFolder,
}

#[derive(Clone)]
enum WorkspacePendingPlan {
    Move(super::workspace_file_ops::WorkspaceMovePlan),
    Create(super::workspace_file_ops::WorkspaceCreatePlan),
}

#[derive(Clone)]
enum WorkspaceUndoOperation {
    Move(super::workspace_file_ops::WorkspaceMovePlan),
    Create(super::workspace_file_ops::WorkspaceCreatePlan),
}

struct WorkspaceOperationDialog {
    kind: WorkspaceOperationKind,
    source: PathBuf,
    input: Entity<Block>,
    plan: Option<WorkspacePendingPlan>,
    error: Option<String>,
    running: bool,
}

#[derive(Clone)]
struct WorkspaceDragPayload {
    path: PathBuf,
    label: String,
    background: Hsla,
    text: Hsla,
}

struct WorkspaceDragPreview {
    payload: WorkspaceDragPayload,
    position: Point<Pixels>,
}

impl Render for WorkspaceDragPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .left(self.position.x + px(10.0))
            .top(self.position.y + px(10.0))
            .max_w(px(260.0))
            .px(px(10.0))
            .h(px(WORKSPACE_NODE_HEIGHT))
            .flex()
            .items_center()
            .overflow_hidden()
            .truncate()
            .rounded(px(6.0))
            .shadow_md()
            .bg(self.payload.background)
            .text_color(self.payload.text)
            .child(self.payload.label.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WorkspaceTreeKind {
    Directory(PathBuf),
    File(PathBuf),
    Heading { line: usize, level: u8 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct WorkspaceTreeNode {
    id: String,
    label: String,
    kind: WorkspaceTreeKind,
    children: Vec<WorkspaceTreeNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum WorkspaceSelection {
    File(PathBuf),
    Outline(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum WorkspaceKeyboardZone {
    #[default]
    Tabs,
    Body,
    SearchOptions,
    SearchResults,
}

#[derive(Clone)]
struct WorkspaceKeyboardNode {
    id: String,
    kind: WorkspaceTreeKind,
    has_children: bool,
    parent_id: Option<String>,
}

#[derive(Default)]
pub(super) struct WorkspaceState {
    pub(super) is_open: bool,
    /// `None` preserves the product default: docked workspace visible on wide windows.
    docked_open_preference: Option<bool>,
    /// Tracks breakpoint transitions without treating compact overlay visibility as a preference.
    compact_layout: Option<bool>,
    active_tab: WorkspaceTab,
    tooltip_hovered: Option<&'static str>,
    tooltip_visible: Option<&'static str>,
    tooltip_task: Option<Task<()>>,
    root: Option<PathBuf>,
    explicit_root: Option<PathBuf>,
    file_tree: Option<WorkspaceTreeNode>,
    file_error: Option<String>,
    file_scan_task: Option<Task<()>>,
    file_scan_generation: u64,
    file_scanning: bool,
    outline_tree: Vec<WorkspaceTreeNode>,
    outline_source: Option<String>,
    outline_revision: Option<(u64, gmark_document::Revision)>,
    outline_requested_source: Option<String>,
    outline_requested_revision: Option<(u64, gmark_document::Revision)>,
    outline_generation: u64,
    outline_task: Option<Task<()>>,
    outline_running: bool,
    expanded: HashSet<String>,
    pinned_empty_directories: HashSet<PathBuf>,
    selected: Option<WorkspaceSelection>,
    search_input: Option<Entity<Block>>,
    search_options: WorkspaceSearchOptions,
    search_results: Vec<WorkspaceSearchMatch>,
    search_error: Option<String>,
    search_running: bool,
    search_generation: u64,
    search_task: Option<Task<()>>,
    pending_navigation: Option<(PathBuf, usize)>,
    operation_dialog: Option<WorkspaceOperationDialog>,
    file_operation_task: Option<Task<()>>,
    file_operation_generation: u64,
    undo_file_operation: Option<WorkspaceUndoOperation>,
    operation_error: Option<String>,
    quick_open: Option<QuickOpenState>,
    focus_handle: Option<FocusHandle>,
    /// Header controls keep stable handles across renders so pointer and Tab
    /// navigation share one visual order instead of recreating focus identity.
    header_focus_handles: Option<[FocusHandle; 3]>,
    keyboard_zone: WorkspaceKeyboardZone,
    search_selected: usize,
    panel_scroll: ScrollHandle,
    panel_width: Option<f32>,
    resize_session: Option<WorkspaceResizeSession>,
    resize_focus_handle: Option<FocusHandle>,
}

#[derive(Clone, Copy, Debug)]
struct WorkspaceResizeSession {
    start_x: Pixels,
    start_width: f32,
    direction: f32,
}

impl Editor {}

#[path = "workspace_parts/controller.rs"]
mod controller;
#[path = "workspace_parts/input.rs"]
mod input;
#[path = "workspace_parts/navigation.rs"]
mod navigation;
#[path = "workspace_parts/operations.rs"]
mod operations;
#[path = "workspace_parts/search_view.rs"]
mod search_view;
#[path = "workspace_parts/tooltip.rs"]
mod tooltip;

use tooltip::render_workspace_tooltip;

pub(super) fn is_markdown_file(path: &Path) -> bool {
    crate::document_io::is_markdown_path(path)
}

fn collect_markdown_paths(node: &WorkspaceTreeNode, paths: &mut Vec<PathBuf>) {
    if let WorkspaceTreeKind::File(path) = &node.kind
        && is_markdown_file(path)
    {
        paths.push(path.clone());
    }
    for child in &node.children {
        collect_markdown_paths(child, paths);
    }
}

fn rank_quick_open_paths(root: &Path, paths: Vec<PathBuf>, query: &str) -> Vec<QuickOpenMatch> {
    let mut matches = paths
        .into_iter()
        .filter_map(|path| {
            let relative_path = path
                .strip_prefix(root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let file_name = path
                .file_name()
                .map(|name| name.to_string_lossy())
                .unwrap_or_default();
            let score = if query.is_empty() {
                0
            } else {
                let path_score = subsequence_score(&relative_path, query)?;
                let file_score = subsequence_score(&file_name, query)
                    .map(|score| score + 2_000)
                    .unwrap_or(i64::MIN);
                path_score.max(file_score)
            };
            Some(QuickOpenMatch {
                path,
                relative_path,
                score,
            })
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right.score.cmp(&left.score).then_with(|| {
            left.relative_path
                .to_lowercase()
                .cmp(&right.relative_path.to_lowercase())
        })
    });
    matches.truncate(QUICK_OPEN_MAX_RESULTS);
    matches
}

pub(super) fn subsequence_score(candidate: &str, query: &str) -> Option<i64> {
    let candidate_chars = candidate.chars().collect::<Vec<_>>();
    let query_chars = query
        .chars()
        .flat_map(char::to_lowercase)
        .collect::<Vec<_>>();
    if query_chars.is_empty() {
        return Some(0);
    }
    let mut query_index = 0usize;
    let mut score = 0i64;
    let mut previous_match = None;
    for (index, ch) in candidate_chars.iter().enumerate() {
        if ch
            .to_lowercase()
            .eq(std::iter::once(query_chars[query_index]))
        {
            score += 100;
            if index == 0
                || candidate_chars
                    .get(index.wrapping_sub(1))
                    .is_some_and(|previous| matches!(*previous, '/' | '\\' | '-' | '_' | ' '))
            {
                score += 60;
            }
            if previous_match == Some(index.saturating_sub(1)) {
                score += 80;
            } else if let Some(previous) = previous_match {
                score -= i64::try_from(index.saturating_sub(previous + 1)).unwrap_or(i64::MAX);
            }
            previous_match = Some(index);
            query_index += 1;
            if query_index == query_chars.len() {
                return Some(score - i64::try_from(candidate_chars.len()).unwrap_or(i64::MAX));
            }
        }
    }
    None
}

impl WorkspaceState {
    /// 返回文件树最近一次扫描的 Markdown 快照；补全不得在每次按键时重新遍历磁盘。
    pub(super) fn markdown_snapshot(&self) -> Option<(PathBuf, Vec<PathBuf>)> {
        fn collect(node: &WorkspaceTreeNode, paths: &mut Vec<PathBuf>) {
            if let WorkspaceTreeKind::File(path) = &node.kind
                && is_markdown_file(path)
            {
                paths.push(path.clone());
            }
            for child in &node.children {
                collect(child, paths);
            }
        }

        let root = self.root.clone()?;
        let tree = self.file_tree.as_ref()?;
        let mut paths = Vec::new();
        collect(tree, &mut paths);
        Some((root, paths))
    }

    #[cfg(test)]
    pub(super) fn install_markdown_snapshot_for_test(
        &mut self,
        root: PathBuf,
        paths: Vec<PathBuf>,
    ) {
        self.root = Some(root.clone());
        self.file_tree = Some(WorkspaceTreeNode {
            id: root.to_string_lossy().to_string(),
            label: root
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| root.to_string_lossy().to_string()),
            kind: WorkspaceTreeKind::Directory(root),
            children: paths
                .into_iter()
                .map(|path| WorkspaceTreeNode {
                    id: path.to_string_lossy().to_string(),
                    label: path
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_default(),
                    kind: WorkspaceTreeKind::File(path),
                    children: Vec::new(),
                })
                .collect(),
        });
    }
}

fn search_workspace(
    root: &Path,
    query: &str,
    options: WorkspaceSearchOptions,
) -> Result<Vec<WorkspaceSearchMatch>, String> {
    let source_pattern = if options.regex {
        query.to_owned()
    } else {
        regex::escape(query)
    };
    let pattern = if options.whole_word {
        format!(r"(?u:\b(?:{source_pattern})\b)")
    } else {
        source_pattern
    };
    let matcher = regex::RegexBuilder::new(&pattern)
        .case_insensitive(!options.case_sensitive)
        .unicode(true)
        .build()
        .map_err(|error| error.to_string())?;
    let mut results = Vec::new();
    let walker = ignore::WalkBuilder::new(root)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .build();

    for entry in walker {
        if results.len() >= SEARCH_MAX_RESULTS {
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if !entry.file_type().is_some_and(|kind| kind.is_file()) || !is_markdown_file(path) {
            continue;
        }
        if entry
            .metadata()
            .map(|metadata| metadata.len() > SEARCH_MAX_FILE_BYTES)
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        let Ok(opened) = crate::document_io::decode_markdown_bytes(&bytes) else {
            continue;
        };
        let content = opened.text;
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        for (line_index, line) in content.lines().enumerate() {
            for found in matcher.find_iter(line) {
                let column = line[..found.start()].chars().count() + 1;
                results.push(WorkspaceSearchMatch {
                    path: path.to_path_buf(),
                    relative_path: relative_path.clone(),
                    line: line_index + 1,
                    column,
                    preview: truncate_search_preview(line),
                });
                if results.len() >= SEARCH_MAX_RESULTS {
                    break;
                }
            }
            if results.len() >= SEARCH_MAX_RESULTS {
                break;
            }
        }
    }
    Ok(results)
}

fn truncate_search_preview(line: &str) -> String {
    const MAX_CHARS: usize = 240;
    let mut chars = line.trim().chars();
    let preview = chars.by_ref().take(MAX_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

fn scan_workspace_dir(path: &Path) -> Result<WorkspaceTreeNode> {
    fs::read_dir(path).with_context(|| format!("failed to read '{}'", path.display()))?;
    let mut root = WorkspaceTreeNode {
        id: file_node_id(path),
        label: file_label(path),
        kind: WorkspaceTreeKind::Directory(path.to_path_buf()),
        children: Vec::new(),
    };
    let walker = ignore::WalkBuilder::new(path)
        .hidden(false)
        .follow_links(false)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .build();
    for entry in walker.filter_map(|entry| entry.ok()) {
        if entry.depth() == 0 || !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        insert_workspace_file(&mut root, path, entry.path());
    }
    sort_workspace_tree(&mut root);
    Ok(root)
}

fn insert_workspace_file(root: &mut WorkspaceTreeNode, base: &Path, file: &Path) {
    let Some(parent) = file.parent() else {
        return;
    };
    let Ok(relative_parent) = parent.strip_prefix(base) else {
        return;
    };
    let mut current = root;
    let mut directory_path = base.to_path_buf();
    for component in relative_parent.components() {
        directory_path.push(component.as_os_str());
        let index = current
            .children
            .iter()
            .position(|node| {
                matches!(&node.kind, WorkspaceTreeKind::Directory(path) if path == &directory_path)
            })
            .unwrap_or_else(|| {
                current.children.push(WorkspaceTreeNode {
                    id: file_node_id(&directory_path),
                    label: file_label(&directory_path),
                    kind: WorkspaceTreeKind::Directory(directory_path.clone()),
                    children: Vec::new(),
                });
                current.children.len() - 1
            });
        current = &mut current.children[index];
    }
    current.children.push(WorkspaceTreeNode {
        id: file_node_id(file),
        label: file_label(file),
        kind: WorkspaceTreeKind::File(file.to_path_buf()),
        children: Vec::new(),
    });
}

fn insert_workspace_directory(root: &mut WorkspaceTreeNode, base: &Path, directory: &Path) {
    let Ok(relative) = directory.strip_prefix(base) else {
        return;
    };
    let mut current = root;
    let mut directory_path = base.to_path_buf();
    for component in relative.components() {
        directory_path.push(component.as_os_str());
        let index = current
            .children
            .iter()
            .position(|node| {
                matches!(&node.kind, WorkspaceTreeKind::Directory(path) if path == &directory_path)
            })
            .unwrap_or_else(|| {
                current.children.push(WorkspaceTreeNode {
                    id: file_node_id(&directory_path),
                    label: file_label(&directory_path),
                    kind: WorkspaceTreeKind::Directory(directory_path.clone()),
                    children: Vec::new(),
                });
                current.children.len() - 1
            });
        current = &mut current.children[index];
    }
}

fn sort_workspace_tree(node: &mut WorkspaceTreeNode) {
    node.children.sort_by(|left, right| {
        let left_dir = matches!(left.kind, WorkspaceTreeKind::Directory(_));
        let right_dir = matches!(right.kind, WorkspaceTreeKind::Directory(_));
        right_dir
            .cmp(&left_dir)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });
    for child in &mut node.children {
        sort_workspace_tree(child);
    }
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn file_node_id(path: &Path) -> String {
    format!("file:{}", path.to_string_lossy())
}

fn stable_node_hash(id: &str) -> u64 {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn workspace_panel_width_for_viewport(
    viewport_width: f32,
    preferred_width: Option<f32>,
) -> f32 {
    if workspace_uses_overlay(viewport_width) {
        return WORKSPACE_COMPACT_OVERLAY_WIDTH.min(viewport_width.max(0.0));
    }
    if let Some(width) = preferred_width.filter(|width| width.is_finite()) {
        return width.clamp(WORKSPACE_PANEL_MIN_WIDTH, WORKSPACE_PANEL_MAX_WIDTH);
    }
    let target = viewport_width * WORKSPACE_PANEL_TARGET_RATIO;
    target.clamp(WORKSPACE_PANEL_AUTO_MIN_WIDTH, WORKSPACE_PANEL_MAX_WIDTH)
}

pub(super) fn workspace_uses_overlay(viewport_width: f32) -> bool {
    viewport_width < WORKSPACE_COMPACT_BREAKPOINT
}

fn collect_visible_keyboard_nodes(
    nodes: &[WorkspaceTreeNode],
    expanded: &HashSet<String>,
    parent_id: Option<&str>,
    output: &mut Vec<WorkspaceKeyboardNode>,
) {
    for node in nodes {
        output.push(WorkspaceKeyboardNode {
            id: node.id.clone(),
            kind: node.kind.clone(),
            has_children: !node.children.is_empty(),
            parent_id: parent_id.map(str::to_owned),
        });
        if !node.children.is_empty() && expanded.contains(&node.id) {
            collect_visible_keyboard_nodes(&node.children, expanded, Some(&node.id), output);
        }
    }
}

fn prune_outline_state(workspace: &mut WorkspaceState, outline: &[WorkspaceTreeNode]) {
    let mut current_ids = HashSet::new();
    collect_node_ids(outline, &mut current_ids);
    workspace
        .expanded
        .retain(|id| !is_outline_node_id(id) || current_ids.contains(id));

    if matches!(
        &workspace.selected,
        Some(WorkspaceSelection::Outline(id)) if !current_ids.contains(id)
    ) {
        workspace.selected = None;
    }
}

fn collect_node_ids(nodes: &[WorkspaceTreeNode], ids: &mut HashSet<String>) {
    for node in nodes {
        ids.insert(node.id.clone());
        collect_node_ids(&node.children, ids);
    }
}

fn is_outline_node_id(id: &str) -> bool {
    id.starts_with("outline:")
}

fn build_outline_tree(markdown: &str) -> Vec<WorkspaceTreeNode> {
    let mut roots = Vec::new();
    let mut stack: Vec<(u8, Vec<usize>)> = Vec::new();
    let mut fence: Option<(char, usize)> = None;

    for (line_index, line) in markdown.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some((marker, len)) = fence {
            if is_closing_fence(trimmed, marker, len) {
                fence = None;
            }
            continue;
        }

        if let Some(next_fence) = opening_fence(trimmed) {
            fence = Some(next_fence);
            continue;
        }

        let Some((level, title)) = BlockKind::parse_atx_heading_line(line) else {
            continue;
        };

        while stack
            .last()
            .is_some_and(|(parent_level, _)| *parent_level >= level)
        {
            stack.pop();
        }

        let node = WorkspaceTreeNode {
            id: format!("outline:{line_index}"),
            label: title,
            kind: WorkspaceTreeKind::Heading {
                line: line_index,
                level,
            },
            children: Vec::new(),
        };

        let siblings = if let Some((_, parent_path)) = stack.last() {
            children_at_path_mut(&mut roots, parent_path)
        } else {
            &mut roots
        };
        siblings.push(node);

        let mut node_path = stack
            .last()
            .map(|(_, path)| path.clone())
            .unwrap_or_default();
        node_path.push(siblings.len() - 1);
        stack.push((level, node_path));
    }

    roots
}

fn children_at_path_mut<'a>(
    nodes: &'a mut Vec<WorkspaceTreeNode>,
    path: &[usize],
) -> &'a mut Vec<WorkspaceTreeNode> {
    let mut current = nodes;
    for &index in path {
        current = &mut current[index].children;
    }
    current
}

fn opening_fence(trimmed: &str) -> Option<(char, usize)> {
    let marker = trimmed.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let len = trimmed.chars().take_while(|ch| *ch == marker).count();
    (len >= 3).then_some((marker, len))
}

fn is_closing_fence(trimmed: &str, marker: char, len: usize) -> bool {
    let count = trimmed.chars().take_while(|ch| *ch == marker).count();
    count >= len && trimmed[count..].trim().is_empty()
}

#[cfg(test)]
#[path = "../../tests/unit/editor/workspace.rs"]
mod tests;
