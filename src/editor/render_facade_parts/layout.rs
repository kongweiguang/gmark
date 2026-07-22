// @author kongweiguang

use super::*;

pub(in crate::editor) fn clamped_floating_panel_origin(
    requested: Point<Pixels>,
    panel_width: f32,
    panel_height: f32,
    viewport: Size<Pixels>,
) -> Point<Pixels> {
    let max_x = (f32::from(viewport.width) - panel_width - FLOATING_PANEL_MARGIN)
        .max(FLOATING_PANEL_MARGIN);
    let max_y = (f32::from(viewport.height) - panel_height - FLOATING_PANEL_MARGIN)
        .max(FLOATING_PANEL_MARGIN);
    point(
        px(f32::from(requested.x).clamp(FLOATING_PANEL_MARGIN, max_x)),
        px(f32::from(requested.y).clamp(FLOATING_PANEL_MARGIN, max_y)),
    )
}

pub(in crate::editor) fn floating_submenu_x(
    parent_x: Pixels,
    parent_width: f32,
    submenu_width: f32,
    gap: f32,
    viewport_width: Pixels,
) -> Pixels {
    let right = f32::from(parent_x) + parent_width + gap;
    if right + submenu_width + FLOATING_PANEL_MARGIN <= f32::from(viewport_width) {
        px(right)
    } else {
        px((f32::from(parent_x) - gap - submenu_width).max(FLOATING_PANEL_MARGIN))
    }
}

pub(super) fn menu_shortcut_text(window: &Window, action: &dyn Action) -> Option<String> {
    let binding = preferred_menu_binding(window.bindings_for_action(action)).or_else(|| {
        // 菜单浮层会接管焦点，不能只按浮层的上下文查询，否则编辑命令看起来像是没有快捷键。
        let editor_context = KeyContext::parse("BlockEditor").expect("known editor key context");
        preferred_menu_binding(window.bindings_for_action_in_context(action, editor_context))
    })?;
    Some(
        binding
            .keystrokes()
            .iter()
            .map(menu_keystroke_text)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn preferred_menu_binding(bindings: Vec<KeyBinding>) -> Option<KeyBinding> {
    // 默认表同时注册跨平台按键。菜单应优先展示当前平台惯用的那一个，避免 Windows 显示 Win+G。
    #[cfg(target_os = "macos")]
    let preferred = bindings.iter().rfind(|binding| {
        binding
            .keystrokes()
            .iter()
            .any(|keystroke| keystroke.modifiers().platform)
    });
    #[cfg(not(target_os = "macos"))]
    let preferred = bindings.iter().rfind(|binding| {
        binding
            .keystrokes()
            .iter()
            .all(|keystroke| !keystroke.modifiers().platform)
    });
    preferred
        .cloned()
        .or_else(|| bindings.into_iter().next_back())
}

fn menu_keystroke_text(keystroke: &KeybindingKeystroke) -> String {
    let modifiers = keystroke.modifiers();
    let mut parts = Vec::with_capacity(6);
    if modifiers.control {
        parts.push("Ctrl".to_owned());
    }
    if modifiers.alt {
        parts.push("Alt".to_owned());
    }
    if modifiers.shift {
        parts.push("Shift".to_owned());
    }
    if modifiers.platform {
        #[cfg(target_os = "macos")]
        parts.push("Cmd".to_owned());
        #[cfg(target_os = "windows")]
        parts.push("Win".to_owned());
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        parts.push("Super".to_owned());
    }
    if modifiers.function {
        parts.push("Fn".to_owned());
    }
    parts.push(menu_key_text(keystroke.key()));
    parts.join("+")
}

fn menu_key_text(key: &str) -> String {
    if key.len() == 1
        || key.strip_prefix('f').is_some_and(|suffix| {
            !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit())
        })
    {
        key.to_uppercase()
    } else {
        match key {
            "pageup" => "Page Up".to_owned(),
            "pagedown" => "Page Down".to_owned(),
            "backspace" => "Backspace".to_owned(),
            "delete" => "Delete".to_owned(),
            "escape" => "Esc".to_owned(),
            "enter" => "Enter".to_owned(),
            "space" => "Space".to_owned(),
            "tab" => "Tab".to_owned(),
            "left" => "Left".to_owned(),
            "right" => "Right".to_owned(),
            "up" => "Up".to_owned(),
            "down" => "Down".to_owned(),
            "home" => "Home".to_owned(),
            "end" => "End".to_owned(),
            other => other.to_owned(),
        }
    }
}

pub(super) fn menu_shortcut_slot(text: String, theme: &Theme) -> Div {
    div()
        .ml(px(MENU_SHORTCUT_GAP))
        .min_w(px(MENU_SHORTCUT_SLOT))
        .max_w(px(100.0))
        .flex_shrink_0()
        .overflow_hidden()
        .truncate()
        .text_align(TextAlign::Right)
        .text_size(px((theme.dimensions.menu_text_size - 1.0).max(10.0)))
        .text_color(theme.colors.dialog_muted)
        .child(text)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum DialogButtonKind {
    Secondary,
    Primary,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::editor) enum DialogTitleIcon {
    Files,
    Info,
    Refresh,
    Source,
    Table,
    Warning,
}

impl DialogTitleIcon {
    pub(super) fn path(self) -> &'static str {
        match self {
            Self::Files => "icon/ui/files.svg",
            Self::Info => "icon/ui/info.svg",
            Self::Refresh => "icon/ui/refresh.svg",
            Self::Source => "icon/ui/source.svg",
            Self::Table => "icon/ui/table.svg",
            Self::Warning => "icon/ui/triangle-alert.svg",
        }
    }

    pub(super) fn color(self, theme: &Theme) -> Hsla {
        match self {
            Self::Warning => theme.colors.callout_warning_border,
            _ => theme.colors.text_link,
        }
    }
}

/// 标准对话框只共享视觉约束；状态切换和副作用仍由各业务模块绑定。
pub(in crate::editor) fn modal_overlay(id: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.colors.dialog_backdrop)
}

pub(in crate::editor) fn dialog_panel(
    id: &'static str,
    width: f32,
    theme: &Theme,
) -> Stateful<Div> {
    let c = &theme.colors;
    let d = &theme.dimensions;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .w(px(width))
        .max_w(relative(0.92))
        .max_h(relative(0.9))
        .p(px(d.dialog_padding))
        .flex()
        .flex_col()
        .gap(px(d.dialog_gap))
        .bg(c.dialog_surface)
        .border(px(d.dialog_border_width))
        .border_color(c.dialog_border)
        .rounded(px(d.dialog_radius))
        .shadow_lg()
}

pub(in crate::editor) fn dialog_content(id: &'static str, theme: &Theme) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .w_full()
        .flex_1()
        // 自动高度弹窗里 flex 子项不能收缩到 0，否则正文会被操作区分隔线盖住。
        // 复杂弹窗仍可在 90% 窗口高度内滚动，简单确认弹窗则保留稳定阅读高度。
        .min_h(px(72.0))
        .overflow_y_scroll()
        .scrollbar_width(px(0.0))
        .flex()
        .flex_col()
        .gap(px(theme.dimensions.dialog_gap))
}

pub(in crate::editor) fn dialog_title_with_icon(
    id: &'static str,
    label: String,
    icon: DialogTitleIcon,
    theme: &Theme,
) -> Div {
    let c = &theme.colors;
    let t = &theme.typography;
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            div()
                .size(px(22.0))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .debug_selector(move || format!("{id}-icon"))
                .child(
                    svg()
                        .path(icon.path())
                        .size(px(18.0))
                        .text_color(icon.color(theme)),
                ),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex_grow()
                .truncate()
                .debug_selector(move || format!("{id}-label"))
                .text_size(px(t.dialog_title_size))
                .font_weight(t.dialog_title_weight.to_font_weight())
                .text_color(c.dialog_title)
                .child(label),
        )
}

pub(in crate::editor) fn dialog_body(label: String, theme: &Theme) -> Div {
    let c = &theme.colors;
    let t = &theme.typography;
    div()
        .w_full()
        .flex_none()
        .min_w(px(0.0))
        .text_size(px(t.dialog_body_size))
        .font_weight(t.dialog_body_weight.to_font_weight())
        .line_height(rems(t.text_line_height))
        .text_color(c.dialog_body)
        .child(label)
}

pub(in crate::editor) fn dialog_actions(theme: &Theme) -> Div {
    let c = &theme.colors;
    let d = &theme.dimensions;
    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_wrap()
        .justify_end()
        .gap(px(d.dialog_button_gap))
        .pt(px(12.0))
        .border_t(px(d.dialog_border_width))
        .border_color(c.dialog_border)
}

pub(in crate::editor) fn dialog_button(
    id: &'static str,
    label: String,
    kind: DialogButtonKind,
    theme: &Theme,
) -> Stateful<Div> {
    let c = &theme.colors;
    let d = &theme.dimensions;
    let t = &theme.typography;
    let (background, hover, text, bordered) = match kind {
        DialogButtonKind::Secondary => (
            c.dialog_secondary_button_bg,
            c.dialog_secondary_button_hover,
            c.dialog_secondary_button_text,
            true,
        ),
        DialogButtonKind::Primary => (
            c.dialog_primary_button_bg,
            c.dialog_primary_button_hover,
            c.dialog_primary_button_text,
            false,
        ),
        DialogButtonKind::Danger => (
            c.dialog_danger_button_bg,
            c.dialog_danger_button_hover,
            c.dialog_danger_button_text,
            false,
        ),
    };
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .min_w(px(72.0))
        .h(px(d.dialog_button_height))
        .px(px(d.dialog_button_padding_x))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px((d.dialog_radius - 4.0).max(0.0)))
        .border(px(if bordered { d.dialog_border_width } else { 0.0 }))
        .border_color(c.dialog_border)
        .bg(background)
        .hover(move |this| this.bg(hover))
        .active(|this| this.opacity(0.92))
        .cursor_pointer()
        .flex_shrink_0()
        .whitespace_nowrap()
        .text_size(px(t.dialog_button_size))
        .font_weight(t.dialog_button_weight.to_font_weight())
        .text_color(text)
        .child(label)
}

pub(crate) fn open_about_github_url(cx: &mut App) {
    cx.open_url(ABOUT_GITHUB_URL);
}

pub(super) fn editor_text_font(cx: &App) -> Font {
    let configured = EditorSettings::editor_font_family(cx);
    editor_text_font_for_family(&configured)
}

pub(super) fn editor_text_font_for_family(configured: &str) -> Font {
    // FontFallbacks is internally `Arc<Vec<String>>` — building it once
    // per process and Arc-cloning per render is the right shape, since
    // editor_text_font() is called from Editor::render on every frame.
    static FALLBACKS: std::sync::OnceLock<FontFallbacks> = std::sync::OnceLock::new();
    let fallbacks = FALLBACKS
        .get_or_init(|| {
            let mut families = vec![".SystemUIFont".to_string()];
            families.extend(tibetan_font_fallbacks_for_target_os(std::env::consts::OS));
            FontFallbacks::from_fonts(families)
        })
        .clone();
    let family: SharedString = if configured.is_empty() {
        ".SystemUIFont".into()
    } else {
        configured.to_string().into()
    };
    let mut font = font(family);
    font.fallbacks = Some(fallbacks);
    font
}

pub(super) fn tibetan_font_fallbacks_for_target_os(target_os: &str) -> Vec<String> {
    let families = match target_os {
        "windows" => &[
            "Microsoft Himalaya",
            "Noto Serif Tibetan",
            "Noto Sans Tibetan",
            "BabelStone Tibetan",
        ][..],
        "macos" => &["Kailasa", "Noto Serif Tibetan", "Noto Sans Tibetan"][..],
        _ => &[
            "Noto Serif Tibetan",
            "Noto Sans Tibetan",
            "Microsoft Himalaya",
            "Kailasa",
            "BabelStone Tibetan",
        ][..],
    };
    families
        .iter()
        .map(|family| (*family).to_string())
        .collect()
}

/// Adjacent-row metadata used to collapse spacing inside visual groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RenderedRowSpacingInfo {
    pub(super) quote_group_anchor: Option<uuid::Uuid>,
    pub(super) visible_quote_group_anchor: Option<uuid::Uuid>,
    pub(super) callout_anchor: Option<uuid::Uuid>,
    pub(super) callout_variant: Option<CalloutVariant>,
    pub(super) is_callout_header: bool,
    pub(super) footnote_anchor: Option<uuid::Uuid>,
    pub(super) is_footnote_header: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RenderedRowKind {
    Plain,
    Footnote,
    Callout(CalloutVariant),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RenderedRowDescriptor {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) top_gap: f32,
    pub(super) kind: RenderedRowKind,
}

pub(in crate::editor) struct RenderedRowCache {
    pub(super) visible_len: usize,
    pub(super) first_id: Option<EntityId>,
    pub(super) last_id: Option<EntityId>,
    pub(super) block_gap: f32,
    pub(super) rows: std::sync::Arc<[RenderedRowDescriptor]>,
}

impl RenderedRowCache {
    pub(super) fn matches(&self, visible: &[super::tree::VisibleBlock], block_gap: f32) -> bool {
        self.visible_len == visible.len()
            && self.first_id == visible.first().map(|block| block.entity.entity_id())
            && self.last_id == visible.last().map(|block| block.entity.entity_id())
            && (self.block_gap - block_gap).abs() < f32::EPSILON
    }
}

impl RenderedRowSpacingInfo {
    pub(super) fn from_block(block: &Block) -> Self {
        Self {
            quote_group_anchor: block.quote_group_anchor,
            visible_quote_group_anchor: block.visible_quote_group_anchor,
            callout_anchor: block.callout_anchor,
            callout_variant: block.callout_variant,
            is_callout_header: block.kind().is_callout(),
            footnote_anchor: block.footnote_anchor,
            is_footnote_header: block.kind().is_footnote_definition(),
        }
    }
}

pub(super) fn rendered_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    default_gap: f32,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.quote_group_anchor.is_some()
        && previous.quote_group_anchor == current.quote_group_anchor
    {
        0.0
    } else {
        default_gap
    }
}

pub(super) fn callout_colors(variant: CalloutVariant, theme: &Theme) -> (Hsla, Hsla) {
    let c = &theme.colors;
    match variant {
        CalloutVariant::Note => (c.callout_note_border, c.callout_note_bg),
        CalloutVariant::Tip => (c.callout_tip_border, c.callout_tip_bg),
        CalloutVariant::Important => (c.callout_important_border, c.callout_important_bg),
        CalloutVariant::Warning => (c.callout_warning_border, c.callout_warning_bg),
        CalloutVariant::Caution => (c.callout_caution_border, c.callout_caution_bg),
    }
}

pub(super) fn callout_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    current: RenderedRowSpacingInfo,
    dimensions: &ThemeDimensions,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.visible_quote_group_anchor.is_some()
        && previous.visible_quote_group_anchor == current.visible_quote_group_anchor
    {
        return 0.0;
    }

    if previous.is_callout_header {
        dimensions.callout_header_margin_bottom
    } else {
        dimensions.callout_body_gap
    }
}

pub(super) fn footnote_row_top_gap(
    previous: Option<RenderedRowSpacingInfo>,
    default_gap: f32,
) -> f32 {
    let Some(previous) = previous else {
        return 0.0;
    };

    if previous.is_footnote_header {
        default_gap * 0.75
    } else {
        default_gap
    }
}

pub(super) fn is_wide_menu_char(ch: char) -> bool {
    matches!(
        ch as u32,
        0x1100..=0x11ff
            | 0x2e80..=0xa4cf
            | 0xac00..=0xd7a3
            | 0xf900..=0xfaff
            | 0xfe10..=0xfe6f
            | 0xff00..=0xff60
            | 0xffe0..=0xffe6
    )
}

pub(super) fn estimated_menu_label_width(label: &str, text_size: f32) -> f32 {
    label
        .chars()
        .map(|ch| {
            if ch.is_ascii_whitespace() {
                text_size * 0.35
            } else if ch.is_ascii_punctuation() {
                text_size * 0.45
            } else if ch.is_ascii() {
                text_size * 0.62
            } else if is_wide_menu_char(ch) {
                text_size
            } else {
                text_size * 0.85
            }
        })
        .sum()
}

pub(super) fn menu_bar_button_width(label: &str, dimensions: &ThemeDimensions) -> f32 {
    let content_width = estimated_menu_label_width(label, dimensions.menu_text_size)
        + dimensions.menu_bar_button_padding_x * 2.0;
    dimensions.menu_bar_button_width.max(content_width.ceil())
}

pub(super) fn top_level_menu_button_width(
    index: usize,
    label: &str,
    dimensions: &ThemeDimensions,
) -> f32 {
    if index == 0 && label == "gmark" {
        dimensions.status_bar_height
    } else {
        menu_bar_button_width(label, dimensions)
    }
}

pub(super) fn visible_menu_button_count(menu_expanded: bool, menu_count: usize) -> usize {
    // 一级导航是文档工具的稳定定位点，不随下拉面板或窗口焦点收纳。
    // 保留参数是为了兼容现有状态和测试调用方；其折叠语义仅适用于旧版本。
    let _ = menu_expanded;
    menu_count
}

pub(super) fn supports_in_window_menu_for_target_os(target_os: &str) -> bool {
    target_os != "macos"
}

pub(in crate::editor) fn supports_in_window_menu() -> bool {
    supports_in_window_menu_for_target_os(std::env::consts::OS)
}

#[cfg(test)]
pub(super) fn menu_panel_left<S: AsRef<str>>(
    open_index: usize,
    menu_labels: &[S],
    dimensions: &ThemeDimensions,
) -> f32 {
    menu_panel_left_from_origin(0.0, open_index, menu_labels, dimensions)
}

pub(super) fn menu_panel_left_from_origin<S: AsRef<str>>(
    origin_x: f32,
    open_index: usize,
    menu_labels: &[S],
    dimensions: &ThemeDimensions,
) -> f32 {
    let prior_width: f32 = menu_labels
        .iter()
        .take(open_index)
        .enumerate()
        .map(|(index, label)| top_level_menu_button_width(index, label.as_ref(), dimensions))
        .sum();
    origin_x
        + dimensions.menu_bar_padding_x
        + prior_width
        + dimensions.menu_bar_gap * open_index as f32
}

pub(super) fn menu_panel_width_for_labels<S: AsRef<str>>(
    labels: &[S],
    dimensions: &ThemeDimensions,
) -> f32 {
    let widest_label = labels
        .iter()
        .map(|label| estimated_menu_label_width(label.as_ref(), dimensions.menu_text_size))
        .fold(0.0, f32::max);
    let content_width = widest_label
        + dimensions.menu_item_padding_x * 2.0
        + MENU_SHORTCUT_SLOT
        + MENU_SHORTCUT_GAP;
    dimensions.menu_panel_width.max(content_width.ceil())
}

pub(super) fn owned_menu_item_labels(items: &[OwnedMenuItem]) -> Vec<String> {
    items
        .iter()
        .filter_map(|item| match item {
            OwnedMenuItem::Action { name, .. } => Some(name.to_string()),
            OwnedMenuItem::Submenu(menu) => Some(menu.name.to_string()),
            OwnedMenuItem::SystemMenu(menu) => Some(menu.name.to_string()),
            OwnedMenuItem::Separator => None,
        })
        .collect()
}

pub(super) fn menu_item_visual_height(item: &OwnedMenuItem, dimensions: &ThemeDimensions) -> f32 {
    match item {
        OwnedMenuItem::Separator => {
            dimensions.menu_separator_height + dimensions.menu_separator_margin_y * 2.0
        }
        OwnedMenuItem::Action { .. } | OwnedMenuItem::Submenu(_) | OwnedMenuItem::SystemMenu(_) => {
            dimensions.menu_item_height
        }
    }
}

pub(super) const SCROLLABLE_IMPORT_MENU_VISIBLE_ITEMS: usize = 12;
// 收起态只显示应用菜单图标；展开后一级菜单从其右侧延伸，不保留项目名槽。
pub(super) const INTEGRATED_MENU_LEFT: f32 = -10.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct InWindowMenuChromeLayout {
    pub(super) integrated: bool,
    pub(super) content_height: f32,
    pub(super) bar_top: f32,
    pub(super) bar_height: f32,
    pub(super) origin_x: f32,
    pub(super) panel_top_offset: f32,
}

pub(super) fn in_window_menu_chrome_layout(
    target_os: &str,
    has_menus: bool,
    focus_mode: bool,
    titlebar_height: f32,
    dimensions: &ThemeDimensions,
) -> InWindowMenuChromeLayout {
    let supported = supports_in_window_menu_for_target_os(target_os);
    let visible = has_menus && supported && !focus_mode;
    // 客户端标题栏存在时，菜单复用同一行；服务端装饰仍保留独立菜单行。
    let integrated = visible && titlebar_height > 0.0;
    let content_height = if visible && !integrated {
        dimensions.menu_bar_height
    } else {
        0.0
    };
    let bar_top = if integrated { 0.0 } else { titlebar_height };
    let bar_height = if integrated {
        titlebar_height
    } else if visible {
        dimensions.menu_bar_height
    } else {
        0.0
    };
    let origin_x = if integrated {
        INTEGRATED_MENU_LEFT
    } else {
        0.0
    };
    let panel_top_offset = if integrated {
        (titlebar_height - dimensions.menu_panel_top).max(0.0)
    } else {
        titlebar_height
    };

    InWindowMenuChromeLayout {
        integrated,
        content_height,
        bar_top,
        bar_height,
        origin_x,
        panel_top_offset,
    }
}

pub(super) fn menu_items_visual_height_with_gaps(
    items: &[OwnedMenuItem],
    dimensions: &ThemeDimensions,
) -> f32 {
    if items.is_empty() {
        return 0.0;
    }

    let items_height: f32 = items
        .iter()
        .map(|item| menu_item_visual_height(item, dimensions))
        .sum();
    items_height + dimensions.menu_panel_gap * items.len().saturating_sub(1) as f32
}

pub(super) fn import_menu_split_index(items: &[OwnedMenuItem]) -> Option<usize> {
    let [
        prefix @ ..,
        OwnedMenuItem::Separator,
        OwnedMenuItem::Action { action, .. },
    ] = items
    else {
        return None;
    };

    if action.as_ref().as_any().is::<AddThemeConfig>()
        || action.as_ref().as_any().is::<AddLanguageConfig>()
    {
        Some(prefix.len())
    } else {
        None
    }
}

pub(super) fn scrollable_import_menu_scroll_height(
    scroll_items: &[OwnedMenuItem],
    footer_items: &[OwnedMenuItem],
    viewport_height: f32,
    top_offset: f32,
    dimensions: &ThemeDimensions,
) -> f32 {
    let visible_count = scroll_items.len().min(SCROLLABLE_IMPORT_MENU_VISIBLE_ITEMS);
    if visible_count == 0 {
        return 0.0;
    }

    let default_height =
        menu_items_visual_height_with_gaps(&scroll_items[..visible_count], dimensions);
    let footer_height = menu_items_visual_height_with_gaps(footer_items, dimensions);
    let footer_gap = if footer_items.is_empty() {
        0.0
    } else {
        dimensions.menu_panel_gap
    };
    let available_height = viewport_height
        - top_offset
        - dimensions.menu_panel_top
        - dimensions.menu_panel_padding * 2.0
        - footer_height
        - footer_gap
        - 8.0;
    let min_height = dimensions.menu_item_height.min(default_height).max(1.0);

    default_height.min(available_height.max(min_height))
}
