// @author kongweiguang

//! Flowchart-specific layout normalization and SVG edge smoothing.

pub(super) const MERMAID_FLOWCHART_SPACING: f32 = 64.0;
pub(super) const MERMAID_ARROW_TIP_EXTENSION: f32 = 5.0;

pub(super) fn normalize_flowchart_decision_branches(
    layout: &mut mermaid_rs_renderer::Layout,
    direction: FlowchartDirection,
) {
    if layout.kind != mermaid_rs_renderer::DiagramKind::Flowchart {
        return;
    }
    let nodes = layout.nodes.clone();
    for (index, edge) in layout.edges.iter_mut().enumerate() {
        let (Some(source), Some(target)) = (nodes.get(&edge.from), nodes.get(&edge.to)) else {
            continue;
        };
        if source.shape != mermaid_rs_renderer::NodeShape::Diamond {
            continue;
        }

        let source_center = (
            source.x + source.width / 2.0,
            source.y + source.height / 2.0,
        );
        let target_center = (
            target.x + target.width / 2.0,
            target.y + target.height / 2.0,
        );
        let side = match direction {
            FlowchartDirection::TopDown | FlowchartDirection::BottomUp => {
                branch_side(target_center.0 - source_center.0, index)
            }
            FlowchartDirection::LeftRight | FlowchartDirection::RightLeft => {
                branch_side(target_center.1 - source_center.1, index)
            }
        };
        let tip_gap = if edge.arrow_end {
            MERMAID_ARROW_TIP_EXTENSION
        } else {
            0.0
        };
        let (start, end) = match direction {
            FlowchartDirection::TopDown => (
                (
                    source_center.0 + side * source.width * 0.25,
                    source.y + source.height * 0.75,
                ),
                (target_center.0, target.y - tip_gap),
            ),
            FlowchartDirection::BottomUp => (
                (
                    source_center.0 + side * source.width * 0.25,
                    source.y + source.height * 0.25,
                ),
                (target_center.0, target.y + target.height + tip_gap),
            ),
            FlowchartDirection::LeftRight => (
                (
                    source.x + source.width * 0.75,
                    source_center.1 + side * source.height * 0.25,
                ),
                (target.x - tip_gap, target_center.1),
            ),
            FlowchartDirection::RightLeft => (
                (
                    source.x + source.width * 0.25,
                    source_center.1 + side * source.height * 0.25,
                ),
                (target.x + target.width + tip_gap, target_center.1),
            ),
        };
        let primary_gap = match direction {
            FlowchartDirection::TopDown | FlowchartDirection::BottomUp => (end.1 - start.1).abs(),
            FlowchartDirection::LeftRight | FlowchartDirection::RightLeft => {
                (end.0 - start.0).abs()
            }
        };
        let label_gap = (primary_gap * 0.33).clamp(28.0, 64.0);
        let tail = (primary_gap * 0.12).clamp(10.0, 18.0);
        let (label_anchor, last_port) = match direction {
            FlowchartDirection::TopDown => ((end.0, end.1 - label_gap), (end.0, end.1 - tail)),
            FlowchartDirection::BottomUp => ((end.0, end.1 + label_gap), (end.0, end.1 + tail)),
            FlowchartDirection::LeftRight => ((end.0 - label_gap, end.1), (end.0 - tail, end.1)),
            FlowchartDirection::RightLeft => ((end.0 + label_gap, end.1), (end.0 + tail, end.1)),
        };

        edge.points = vec![start, label_anchor, last_port, end];
        if edge.label.is_some() {
            edge.label_anchor = Some(label_anchor);
        }
    }
}

fn branch_side(delta: f32, edge_index: usize) -> f32 {
    if delta.abs() > f32::EPSILON {
        delta.signum()
    } else if edge_index.is_multiple_of(2) {
        -1.0
    } else {
        1.0
    }
}

pub(super) fn mermaid_render_options(source: &str) -> mermaid_rs_renderer::RenderOptions {
    let mut options = mermaid_rs_renderer::RenderOptions::modern();
    if flowchart_direction(source).is_some() {
        options = options
            .with_node_spacing(MERMAID_FLOWCHART_SPACING)
            .with_rank_spacing(MERMAID_FLOWCHART_SPACING);
    }
    // 网格路由会把短分支挤进同一条狭窄通道，圆角化后形成明显的 S 形回折。
    // 普通分层路由更接近 Mermaid JS：分支从判断节点自然展开，且仍避让节点。
    options.layout.flowchart.routing.enable_grid_router = false;
    options
}

#[derive(Clone, Copy)]
pub(super) enum FlowchartDirection {
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

pub(super) fn flowchart_direction(source: &str) -> Option<FlowchartDirection> {
    let header = source.lines().find_map(|line| {
        let line = line.trim();
        (!line.is_empty() && !line.starts_with("%%")).then_some(line)
    })?;
    let mut parts = header.split_whitespace();
    let kind = parts.next()?;
    if !kind.eq_ignore_ascii_case("flowchart") && !kind.eq_ignore_ascii_case("graph") {
        return None;
    }
    Some(
        match parts.next().unwrap_or("TD").to_ascii_uppercase().as_str() {
            "BT" => FlowchartDirection::BottomUp,
            "LR" => FlowchartDirection::LeftRight,
            "RL" => FlowchartDirection::RightLeft,
            _ => FlowchartDirection::TopDown,
        },
    )
}

pub(super) fn smooth_forward_flowchart_edges(source: &str, svg: &str) -> String {
    let Some(direction) = flowchart_direction(source) else {
        return svg.to_string();
    };
    let mut output = String::with_capacity(svg.len());
    let mut cursor = 0;
    while let Some(relative_start) = svg[cursor..].find("<path") {
        let start = cursor + relative_start;
        output.push_str(&svg[cursor..start]);
        let Some(relative_end) = svg[start..].find('>') else {
            output.push_str(&svg[start..]);
            return output;
        };
        let end = start + relative_end + 1;
        let path = &svg[start..end];
        output.push_str(&smooth_flowchart_edge_path(path, direction));
        cursor = end;
    }
    output.push_str(&svg[cursor..]);
    output
}

fn smooth_flowchart_edge_path(path: &str, direction: FlowchartDirection) -> String {
    if !path.contains("class=\"edgePath\"") || !path.contains(" Q ") {
        return path.to_string();
    }
    let Some(d_start) = path.find(" d=\"").map(|index| index + 4) else {
        return path.to_string();
    };
    let Some(d_len) = path[d_start..].find('"') else {
        return path.to_string();
    };
    let d_end = d_start + d_len;
    let points = flowchart_path_endpoints(&path[d_start..d_end]);
    if points.len() < 4 {
        return path.to_string();
    }
    let (Some(start), Some(first_port), Some(last_port), Some(end)) = (
        points.first(),
        points.get(1),
        points.get(points.len().saturating_sub(2)),
        points.last(),
    ) else {
        return path.to_string();
    };
    let forward = match direction {
        FlowchartDirection::TopDown => end[1] > start[1],
        FlowchartDirection::BottomUp => end[1] < start[1],
        FlowchartDirection::LeftRight => end[0] > start[0],
        FlowchartDirection::RightLeft => end[0] < start[0],
    };
    if !forward {
        return path.to_string();
    }

    let overall = [end[0] - start[0], end[1] - start[1]];
    let end_tangent = [end[0] - last_port[0], end[1] - last_port[1]];
    let Some(overall_unit) = normalized_vector(overall) else {
        return path.to_string();
    };
    let Some(end_tangent_unit) = normalized_vector(end_tangent) else {
        return path.to_string();
    };
    let start_tangent = if path.contains("marker-start=") {
        normalized_vector([first_port[0] - start[0], first_port[1] - start[1]])
            .unwrap_or(overall_unit)
    } else {
        overall_unit
    };
    let handle = vector_length(overall).mul_add(0.32, 0.0).clamp(12.0, 80.0);
    let first_control = [
        start[0] + start_tangent[0] * handle,
        start[1] + start_tangent[1] * handle,
    ];
    let last_control = [
        end[0] - end_tangent_unit[0] * handle,
        end[1] - end_tangent_unit[1] * handle,
    ];

    // 中段使用连续曲线；末端控制点保持原入射方向，箭头会沿正确切线进入节点。
    let curve = format!(
        "M {:.3},{:.3} C {:.3},{:.3} {:.3},{:.3} {:.3},{:.3}",
        start[0],
        start[1],
        first_control[0],
        first_control[1],
        last_control[0],
        last_control[1],
        end[0],
        end[1]
    );
    format!("{}{}{}", &path[..d_start], curve, &path[d_end..])
}

fn vector_length(vector: [f32; 2]) -> f32 {
    vector[0].hypot(vector[1])
}

fn normalized_vector(vector: [f32; 2]) -> Option<[f32; 2]> {
    let length = vector_length(vector);
    (length.is_finite() && length > f32::EPSILON)
        .then_some([vector[0] / length, vector[1] / length])
}

fn flowchart_path_endpoints(path: &str) -> Vec<[f32; 2]> {
    let normalized = path.replace(',', " ");
    let mut tokens = normalized.split_whitespace();
    let mut points = Vec::new();
    while let Some(command) = tokens.next() {
        let number = |token: Option<&str>| token.and_then(|value| value.parse::<f32>().ok());
        match command {
            "M" | "L" => {
                let (Some(x), Some(y)) = (number(tokens.next()), number(tokens.next())) else {
                    return Vec::new();
                };
                points.push([x, y]);
            }
            "Q" => {
                let (Some(_control_x), Some(_control_y), Some(x), Some(y)) = (
                    number(tokens.next()),
                    number(tokens.next()),
                    number(tokens.next()),
                    number(tokens.next()),
                ) else {
                    return Vec::new();
                };
                points.push([x, y]);
            }
            _ => return Vec::new(),
        }
    }
    points
}
