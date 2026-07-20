// @author kongweiguang

//! Pure command metadata and availability decisions shared by contextual editing UI.

use super::BlockKind;
use gpui::{App, AppContext, Global};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EditingCommandId {
    Paragraph,
    Heading1,
    Heading2,
    Heading3,
    BulletedList,
    NumberedList,
    TaskList,
    Quote,
    CodeBlock,
    Table,
    Image,
    Math,
    HorizontalRule,
    DuplicateBlock,
    MoveBlockUp,
    MoveBlockDown,
    DeleteBlock,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    InlineCode,
    Link,
    ClearFormatting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditingCommandCategory {
    Transform,
    Insert,
    Block,
    Inline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EditingCommandDescriptor {
    pub(crate) id: EditingCommandId,
    pub(crate) category: EditingCommandCategory,
    pub(crate) localization_key: &'static str,
    pub(crate) icon_path: &'static str,
    pub(crate) shortcut: Option<&'static str>,
    pub(crate) aliases: &'static [&'static str],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EditingViewMode {
    #[default]
    Rendered,
    Source,
    Split,
    Preview,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EditingBlockContext {
    #[default]
    RichText,
    TableCell,
    Raw,
    Code,
    Math,
    Structural,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EditingSelectionContext {
    #[default]
    None,
    WithinBlock,
    AcrossBlocks,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct EditingContext {
    pub(crate) view_mode: EditingViewMode,
    pub(crate) block: EditingBlockContext,
    pub(crate) selection: EditingSelectionContext,
    pub(crate) read_only: bool,
    pub(crate) sibling_index: usize,
    pub(crate) sibling_count: usize,
}

impl EditingContext {
    fn editable_block(self) -> bool {
        !self.read_only
            && self.view_mode == EditingViewMode::Rendered
            && self.block != EditingBlockContext::TableCell
            && self.selection != EditingSelectionContext::AcrossBlocks
    }

    fn editable_rich_text(self) -> bool {
        !self.read_only
            && self.view_mode == EditingViewMode::Rendered
            && matches!(
                self.block,
                EditingBlockContext::RichText | EditingBlockContext::TableCell
            )
    }

    fn editable_rich_block(self) -> bool {
        self.editable_rich_text()
            && self.block == EditingBlockContext::RichText
            && self.selection != EditingSelectionContext::AcrossBlocks
    }

    fn has_selection(self) -> bool {
        self.selection != EditingSelectionContext::None
    }

    fn cross_block_selection(self) -> bool {
        self.selection == EditingSelectionContext::AcrossBlocks
    }

    fn can_move_up(self) -> bool {
        self.editable_block() && self.sibling_index > 0
    }

    fn can_move_down(self) -> bool {
        self.editable_block() && self.sibling_index + 1 < self.sibling_count
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EditingCommandPlan {
    ChangeBlockKind(BlockKind),
    InsertTable,
    InsertImage,
    InsertMath,
    InsertHorizontalRule,
    DuplicateBlock,
    MoveBlock(i32),
    DeleteBlock,
    ApplyInline(EditingCommandId),
}

pub(crate) const SLASH_COMMANDS: [EditingCommandId; 17] = [
    EditingCommandId::Paragraph,
    EditingCommandId::Heading1,
    EditingCommandId::Heading2,
    EditingCommandId::Heading3,
    EditingCommandId::BulletedList,
    EditingCommandId::NumberedList,
    EditingCommandId::TaskList,
    EditingCommandId::Quote,
    EditingCommandId::CodeBlock,
    EditingCommandId::Table,
    EditingCommandId::Image,
    EditingCommandId::Math,
    EditingCommandId::HorizontalRule,
    EditingCommandId::DuplicateBlock,
    EditingCommandId::MoveBlockUp,
    EditingCommandId::MoveBlockDown,
    EditingCommandId::DeleteBlock,
];

pub(crate) const TRANSFORM_COMMANDS: [EditingCommandId; 9] = [
    EditingCommandId::Paragraph,
    EditingCommandId::Heading1,
    EditingCommandId::Heading2,
    EditingCommandId::Heading3,
    EditingCommandId::BulletedList,
    EditingCommandId::NumberedList,
    EditingCommandId::TaskList,
    EditingCommandId::Quote,
    EditingCommandId::CodeBlock,
];

/// 插入入口共用这一份清单，避免块操作、右键菜单和斜杠菜单的能力漂移。
pub(crate) const INSERT_COMMANDS: [EditingCommandId; 4] = [
    EditingCommandId::Table,
    EditingCommandId::Image,
    EditingCommandId::Math,
    EditingCommandId::HorizontalRule,
];

pub(crate) const BLOCK_MENU_COMMANDS: [EditingCommandId; 17] = [
    EditingCommandId::Paragraph,
    EditingCommandId::Heading1,
    EditingCommandId::Heading2,
    EditingCommandId::Heading3,
    EditingCommandId::BulletedList,
    EditingCommandId::NumberedList,
    EditingCommandId::TaskList,
    EditingCommandId::Quote,
    EditingCommandId::CodeBlock,
    EditingCommandId::Table,
    EditingCommandId::Image,
    EditingCommandId::Math,
    EditingCommandId::HorizontalRule,
    EditingCommandId::DuplicateBlock,
    EditingCommandId::MoveBlockUp,
    EditingCommandId::MoveBlockDown,
    EditingCommandId::DeleteBlock,
];

pub(crate) const INLINE_COMMANDS: [EditingCommandId; 7] = [
    EditingCommandId::Bold,
    EditingCommandId::Italic,
    EditingCommandId::Underline,
    EditingCommandId::Strikethrough,
    EditingCommandId::InlineCode,
    EditingCommandId::Link,
    EditingCommandId::ClearFormatting,
];

impl EditingCommandId {
    pub(crate) fn stable_id(self) -> &'static str {
        use EditingCommandId::*;
        match self {
            Paragraph => "paragraph",
            Heading1 => "heading_1",
            Heading2 => "heading_2",
            Heading3 => "heading_3",
            BulletedList => "bulleted_list",
            NumberedList => "numbered_list",
            TaskList => "task_list",
            Quote => "quote",
            CodeBlock => "code_block",
            Table => "table",
            Image => "image",
            Math => "math",
            HorizontalRule => "horizontal_rule",
            DuplicateBlock => "duplicate_block",
            MoveBlockUp => "move_block_up",
            MoveBlockDown => "move_block_down",
            DeleteBlock => "delete_block",
            Bold => "bold",
            Italic => "italic",
            Underline => "underline",
            Strikethrough => "strikethrough",
            InlineCode => "inline_code",
            Link => "link",
            ClearFormatting => "clear_formatting",
        }
    }

    pub(crate) fn from_stable_id(id: &str) -> Option<Self> {
        SLASH_COMMANDS
            .into_iter()
            .chain(INLINE_COMMANDS)
            .find(|command| command.stable_id() == id)
    }

    pub(crate) fn for_block_kind(kind: &BlockKind) -> Option<Self> {
        match kind {
            BlockKind::Paragraph => Some(Self::Paragraph),
            BlockKind::Heading { level: 1 } => Some(Self::Heading1),
            BlockKind::Heading { level: 2 } => Some(Self::Heading2),
            BlockKind::Heading { level: 3 } => Some(Self::Heading3),
            BlockKind::BulletedListItem => Some(Self::BulletedList),
            BlockKind::NumberedListItem => Some(Self::NumberedList),
            BlockKind::TaskListItem { .. } => Some(Self::TaskList),
            BlockKind::Quote => Some(Self::Quote),
            BlockKind::CodeBlock { .. } => Some(Self::CodeBlock),
            _ => None,
        }
    }

    pub(crate) fn descriptor(self) -> EditingCommandDescriptor {
        use EditingCommandCategory::{Block, Inline, Insert, Transform};
        use EditingCommandId::*;
        match self {
            Paragraph => descriptor(
                self,
                Transform,
                "paragraph",
                "icon/ui/type.svg",
                &["text", "body", "正文", "zw"],
            ),
            Heading1 => descriptor(
                self,
                Transform,
                "heading_1",
                "icon/ui/heading-1.svg",
                &["heading 1", "h1", "title", "标题 1", "bt1"],
            ),
            Heading2 => descriptor(
                self,
                Transform,
                "heading_2",
                "icon/ui/heading-2.svg",
                &["heading 2", "h2", "subtitle", "标题 2", "bt2"],
            ),
            Heading3 => descriptor(
                self,
                Transform,
                "heading_3",
                "icon/ui/heading-3.svg",
                &["heading 3", "h3", "标题 3", "bt3"],
            ),
            BulletedList => descriptor(
                self,
                Transform,
                "bulleted_list",
                "icon/ui/list.svg",
                &[
                    "bullet list",
                    "unordered list",
                    "列表",
                    "无序列表",
                    "lb",
                    "wxlb",
                ],
            ),
            NumberedList => descriptor(
                self,
                Transform,
                "numbered_list",
                "icon/ui/list-ordered.svg",
                &[
                    "numbered list",
                    "ordered list",
                    "编号列表",
                    "有序列表",
                    "bhlb",
                    "yxlb",
                ],
            ),
            TaskList => descriptor(
                self,
                Transform,
                "task_list",
                "icon/ui/list-checks.svg",
                &["task list", "checklist", "任务列表", "清单", "rwlb", "qd"],
            ),
            Quote => descriptor(
                self,
                Transform,
                "quote",
                "icon/ui/quote.svg",
                &["quote", "blockquote", "引用", "yy"],
            ),
            CodeBlock => descriptor(
                self,
                Transform,
                "code_block",
                "icon/ui/code.svg",
                &["code", "code block", "代码", "代码块", "dm", "dmk"],
            ),
            Table => descriptor(
                self,
                Insert,
                "table",
                "icon/ui/table.svg",
                &["table", "表格", "bg"],
            ),
            Image => descriptor(
                self,
                Insert,
                "image",
                "icon/ui/image.svg",
                &["image", "picture", "图片", "图像", "tp", "tx"],
            ),
            Math => descriptor(
                self,
                Insert,
                "math",
                "icon/ui/sigma.svg",
                &["math", "formula", "equation", "公式", "数学", "gs", "sx"],
            ),
            HorizontalRule => descriptor(
                self,
                Insert,
                "horizontal_rule",
                "icon/ui/minus.svg",
                &[
                    "horizontal rule",
                    "divider",
                    "separator",
                    "分隔线",
                    "水平线",
                    "fgx",
                    "spx",
                ],
            ),
            DuplicateBlock => descriptor(
                self,
                Block,
                "duplicate_block",
                "icon/ui/copy.svg",
                &["duplicate", "copy block", "复制块", "复刻", "fz"],
            ),
            MoveBlockUp => descriptor(
                self,
                Block,
                "move_block_up",
                "icon/ui/arrow-up.svg",
                &["move up", "上移", "sy"],
            ),
            MoveBlockDown => descriptor(
                self,
                Block,
                "move_block_down",
                "icon/ui/arrow-down.svg",
                &["move down", "下移", "xy"],
            ),
            DeleteBlock => descriptor(
                self,
                Block,
                "delete_block",
                "icon/ui/trash.svg",
                &["delete", "remove", "删除块", "sc"],
            ),
            Bold => descriptor(
                self,
                Inline,
                "bold",
                "icon/ui/type.svg",
                &["bold", "粗体", "ct"],
            ),
            Italic => descriptor(
                self,
                Inline,
                "italic",
                "icon/ui/type.svg",
                &["italic", "斜体", "xt"],
            ),
            Underline => descriptor(
                self,
                Inline,
                "underline",
                "icon/ui/type.svg",
                &["underline", "下划线", "xhx"],
            ),
            Strikethrough => descriptor(
                self,
                Inline,
                "strikethrough",
                "icon/ui/type.svg",
                &["strikethrough", "删除线", "scx"],
            ),
            InlineCode => descriptor(
                self,
                Inline,
                "inline_code",
                "icon/ui/code.svg",
                &["inline code", "行内代码", "hndm"],
            ),
            Link => descriptor(
                self,
                Inline,
                "link",
                "icon/ui/link.svg",
                &["link", "链接", "lj"],
            ),
            ClearFormatting => descriptor(
                self,
                Inline,
                "clear_formatting",
                "icon/ui/refresh.svg",
                &["clear formatting", "清除格式", "qcgs"],
            ),
        }
    }

    pub(crate) fn is_available(self, context: EditingContext) -> bool {
        use EditingCommandId::*;
        match self {
            Bold | Italic | Underline | Strikethrough | InlineCode | ClearFormatting => {
                context.editable_rich_text() && context.has_selection()
            }
            Link => {
                context.editable_rich_text()
                    && context.has_selection()
                    && !context.cross_block_selection()
            }
            DuplicateBlock | DeleteBlock => context.editable_block(),
            MoveBlockUp => context.can_move_up(),
            MoveBlockDown => context.can_move_down(),
            Table | Image | Math | HorizontalRule => context.editable_block(),
            Paragraph | Heading1 | Heading2 | Heading3 | BulletedList | NumberedList | TaskList
            | Quote | CodeBlock => context.editable_rich_block(),
        }
    }

    pub(crate) fn plan(self) -> EditingCommandPlan {
        use EditingCommandId::*;
        match self {
            Paragraph => EditingCommandPlan::ChangeBlockKind(BlockKind::Paragraph),
            Heading1 => EditingCommandPlan::ChangeBlockKind(BlockKind::Heading { level: 1 }),
            Heading2 => EditingCommandPlan::ChangeBlockKind(BlockKind::Heading { level: 2 }),
            Heading3 => EditingCommandPlan::ChangeBlockKind(BlockKind::Heading { level: 3 }),
            BulletedList => EditingCommandPlan::ChangeBlockKind(BlockKind::BulletedListItem),
            NumberedList => EditingCommandPlan::ChangeBlockKind(BlockKind::NumberedListItem),
            TaskList => {
                EditingCommandPlan::ChangeBlockKind(BlockKind::TaskListItem { checked: false })
            }
            Quote => EditingCommandPlan::ChangeBlockKind(BlockKind::Quote),
            CodeBlock => {
                EditingCommandPlan::ChangeBlockKind(BlockKind::CodeBlock { language: None })
            }
            Table => EditingCommandPlan::InsertTable,
            Image => EditingCommandPlan::InsertImage,
            Math => EditingCommandPlan::InsertMath,
            HorizontalRule => EditingCommandPlan::InsertHorizontalRule,
            DuplicateBlock => EditingCommandPlan::DuplicateBlock,
            MoveBlockUp => EditingCommandPlan::MoveBlock(-1),
            MoveBlockDown => EditingCommandPlan::MoveBlock(1),
            DeleteBlock => EditingCommandPlan::DeleteBlock,
            Bold | Italic | Underline | Strikethrough | InlineCode | Link | ClearFormatting => {
                EditingCommandPlan::ApplyInline(self)
            }
        }
    }
}

pub(crate) struct EditingCommandHistory {
    recent: Vec<EditingCommandId>,
}

impl Global for EditingCommandHistory {}

fn normalized_recent_commands(ids: &[String]) -> Vec<EditingCommandId> {
    ids.iter()
        .filter_map(|id| EditingCommandId::from_stable_id(id))
        .filter(|command| SLASH_COMMANDS.contains(command))
        .fold(Vec::new(), |mut recent, command| {
            if !recent.contains(&command) && recent.len() < 5 {
                recent.push(command);
            }
            recent
        })
}

fn record_recent_command(recent: &mut Vec<EditingCommandId>, command: EditingCommandId) -> bool {
    if !SLASH_COMMANDS.contains(&command) {
        return false;
    }
    recent.retain(|existing| *existing != command);
    recent.insert(0, command);
    recent.truncate(5);
    true
}

impl EditingCommandHistory {
    pub(crate) fn init(cx: &mut App) {
        let recent = crate::config::read_app_preferences()
            .map(|preferences| normalized_recent_commands(&preferences.recent_editing_commands))
            .unwrap_or_default();
        cx.set_global(Self { recent });
    }

    pub(crate) fn recent(cx: &App) -> Vec<EditingCommandId> {
        cx.try_global::<Self>()
            .map(|history| history.recent.clone())
            .unwrap_or_default()
    }

    pub(crate) fn record(command: EditingCommandId, cx: &mut App) {
        if cx.try_global::<Self>().is_none() {
            cx.set_global(Self { recent: Vec::new() });
        }
        let history = cx.global_mut::<Self>();
        if !record_recent_command(&mut history.recent, command) {
            return;
        }
        let recent = history
            .recent
            .iter()
            .map(|command| command.stable_id().to_owned())
            .collect::<Vec<_>>();
        cx.background_spawn(async move {
            let result = (|| {
                let mut preferences = crate::config::read_app_preferences()?;
                preferences.recent_editing_commands = recent;
                crate::preferences::save_app_preferences(&preferences)
            })();
            if let Err(err) = result {
                eprintln!("failed to persist recent editing commands: {err}");
            }
        })
        .detach();
    }
}

fn descriptor(
    id: EditingCommandId,
    category: EditingCommandCategory,
    localization_key: &'static str,
    icon_path: &'static str,
    aliases: &'static [&'static str],
) -> EditingCommandDescriptor {
    EditingCommandDescriptor {
        id,
        category,
        localization_key,
        icon_path,
        shortcut: match id {
            EditingCommandId::Bold => Some("Mod-B"),
            EditingCommandId::Italic => Some("Mod-I"),
            EditingCommandId::Underline => Some("Mod-U"),
            EditingCommandId::Strikethrough => Some("Mod-Shift-S"),
            EditingCommandId::InlineCode => Some("Mod-E"),
            EditingCommandId::Link => Some("Mod-K"),
            _ => None,
        },
        aliases,
    }
}

pub(crate) fn command_match_score(command: EditingCommandId, query: &str) -> Option<i64> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Some(0);
    }
    let descriptor = command.descriptor();
    descriptor
        .aliases
        .iter()
        .filter_map(|alias| {
            let alias = alias.to_lowercase();
            if alias.starts_with(&query) {
                Some(10_000 - alias.len() as i64)
            } else {
                alias
                    .find(&query)
                    .map(|index| 7_500 - index as i64 - alias.len() as i64)
            }
        })
        .max()
}

pub(crate) fn filter_commands(commands: &[EditingCommandId], query: &str) -> Vec<EditingCommandId> {
    let mut matches = commands
        .iter()
        .copied()
        .filter_map(|command| command_match_score(command, query).map(|score| (command, score)))
        .collect::<Vec<_>>();
    matches.sort_by_key(|item| std::cmp::Reverse(item.1));
    matches.into_iter().map(|(command, _)| command).collect()
}

#[cfg(test)]
#[path = "../../../tests/unit/components/block/editing_command.rs"]
mod tests;
