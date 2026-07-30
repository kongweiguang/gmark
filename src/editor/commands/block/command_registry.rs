// @author kongweiguang

use super::*;

/// 单一命令注册表视图。旧数组暂作为无分配的兼容入口，所有消费者所需的
/// 元数据与 surface 声明都从这里聚合，后续新增命令必须首先进入 `ALL_COMMANDS`。
pub(crate) fn editing_command_specs() -> Vec<EditingCommandSpec> {
    const ALL_COMMANDS: &[EditingCommandId] = &[
        EditingCommandId::Paragraph,
        EditingCommandId::Heading1,
        EditingCommandId::Heading2,
        EditingCommandId::Heading3,
        EditingCommandId::Heading4,
        EditingCommandId::Heading5,
        EditingCommandId::Heading6,
        EditingCommandId::BulletedList,
        EditingCommandId::NumberedList,
        EditingCommandId::TaskList,
        EditingCommandId::Quote,
        EditingCommandId::CodeBlock,
        EditingCommandId::Table,
        EditingCommandId::Image,
        EditingCommandId::Resource,
        EditingCommandId::Math,
        EditingCommandId::Mermaid,
        EditingCommandId::CalloutNote,
        EditingCommandId::CalloutTip,
        EditingCommandId::CalloutImportant,
        EditingCommandId::CalloutWarning,
        EditingCommandId::CalloutCaution,
        EditingCommandId::FootnoteDefinition,
        EditingCommandId::FootnoteReference,
        EditingCommandId::HorizontalRule,
        EditingCommandId::DuplicateBlock,
        EditingCommandId::MoveBlockUp,
        EditingCommandId::MoveBlockDown,
        EditingCommandId::DeleteBlock,
        EditingCommandId::Bold,
        EditingCommandId::Italic,
        EditingCommandId::Underline,
        EditingCommandId::Highlight,
        EditingCommandId::Superscript,
        EditingCommandId::Subscript,
        EditingCommandId::Strikethrough,
        EditingCommandId::InlineCode,
        EditingCommandId::InlineMath,
        EditingCommandId::Link,
        EditingCommandId::ClearFormatting,
    ];
    ALL_COMMANDS
        .iter()
        .copied()
        .map(|id| EditingCommandSpec {
            id,
            descriptor: id.descriptor(),
            surfaces: EditingCommandSurfaces {
                slash: SLASH_COMMANDS.contains(&id),
                block_menu: BLOCK_MENU_COMMANDS.contains(&id),
                transform: TRANSFORM_COMMANDS.contains(&id),
                insert: INSERT_COMMANDS.contains(&id),
                inline: INLINE_COMMANDS.contains(&id),
            },
        })
        .collect()
}

pub(crate) struct EditingCommandHistory {
    recent: Vec<EditingCommandId>,
}

impl Global for EditingCommandHistory {}

pub(super) fn normalized_recent_commands(ids: &[String]) -> Vec<EditingCommandId> {
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

pub(super) fn record_recent_command(
    recent: &mut Vec<EditingCommandId>,
    command: EditingCommandId,
) -> bool {
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

pub(super) fn descriptor(
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
            EditingCommandId::Paragraph => Some("Mod-Alt-0"),
            EditingCommandId::Heading1 => Some("Mod-Alt-1"),
            EditingCommandId::Heading2 => Some("Mod-Alt-2"),
            EditingCommandId::Heading3 => Some("Mod-Alt-3"),
            EditingCommandId::Heading4 => Some("Mod-Alt-4"),
            EditingCommandId::Heading5 => Some("Mod-Alt-5"),
            EditingCommandId::Heading6 => Some("Mod-Alt-6"),
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
