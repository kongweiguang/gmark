// @author kongweiguang

use super::*;

pub(in crate::editor) fn workspace_panel_width_for_viewport(
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

pub(in crate::editor) fn document_sidebar_panel_width_for_viewport(
    viewport_width: f32,
    preferred_width: Option<f32>,
) -> f32 {
    if workspace_uses_overlay(viewport_width) {
        return DOCUMENT_SIDEBAR_COMPACT_OVERLAY_WIDTH.min(viewport_width.max(0.0));
    }
    if let Some(width) = preferred_width.filter(|width| width.is_finite()) {
        return width.clamp(
            DOCUMENT_SIDEBAR_PANEL_MIN_WIDTH,
            DOCUMENT_SIDEBAR_PANEL_MAX_WIDTH,
        );
    }
    (viewport_width * WORKSPACE_PANEL_TARGET_RATIO).clamp(
        DOCUMENT_SIDEBAR_PANEL_AUTO_MIN_WIDTH,
        DOCUMENT_SIDEBAR_PANEL_MAX_WIDTH,
    )
}

pub(in crate::editor) fn workspace_uses_overlay(viewport_width: f32) -> bool {
    viewport_width < WORKSPACE_COMPACT_BREAKPOINT
}

pub(super) fn collect_visible_keyboard_nodes(
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

pub(super) fn prune_outline_state(workspace: &mut WorkspaceState, outline: &[WorkspaceTreeNode]) {
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

pub(super) fn build_outline_tree(markdown: &str) -> Vec<WorkspaceTreeNode> {
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
