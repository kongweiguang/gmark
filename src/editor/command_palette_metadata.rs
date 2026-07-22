// @author kongweiguang

//! Pure user-facing descriptions for command-palette actions.

use gpui::Action;

use super::{canonical_action_id, editing_command_for_action};
use crate::components::EditingCommandId;

pub(super) fn localized_action_description(
    action: &dyn Action,
    label: &str,
    language_id: &str,
) -> String {
    let action_id = canonical_action_id(action.name());
    if language_id.starts_with("zh") {
        if let Some(command) = editing_command_for_action(action) {
            return match command {
                EditingCommandId::Paragraph => "将当前内容转换为普通正文段落",
                EditingCommandId::Heading1 => "将当前段落转换为一级标题",
                EditingCommandId::Heading2 => "将当前段落转换为二级标题",
                EditingCommandId::Heading3 => "将当前段落转换为三级标题",
                EditingCommandId::Heading4 => "将当前段落转换为四级标题",
                EditingCommandId::Heading5 => "将当前段落转换为五级标题",
                EditingCommandId::Heading6 => "将当前段落转换为六级标题",
                EditingCommandId::BulletedList => "将当前段落转换为项目符号列表",
                EditingCommandId::NumberedList => "将当前段落转换为带序号的列表",
                EditingCommandId::TaskList => "将当前段落转换为可勾选的任务列表",
                EditingCommandId::Quote => "将当前段落转换为引用块",
                EditingCommandId::CodeBlock => "将当前段落转换为支持语法高亮的代码块",
                EditingCommandId::Bold => "为所选文字添加或移除粗体格式",
                EditingCommandId::Italic => "为所选文字添加或移除斜体格式",
                EditingCommandId::Underline => "为所选文字添加或移除下划线",
                EditingCommandId::Strikethrough => "为所选文字添加或移除删除线",
                EditingCommandId::Highlight => "为所选文字添加或移除高亮",
                EditingCommandId::Superscript => "将所选文字设为上标或恢复正文",
                EditingCommandId::Subscript => "将所选文字设为下标或恢复正文",
                EditingCommandId::InlineMath => "插入行内数学公式或格式化所选内容",
                EditingCommandId::InlineCode => "为所选文字添加或移除行内代码格式",
                EditingCommandId::Link => "把所选文字转换为链接或编辑现有链接",
                _ => return format!("执行“{label}”命令"),
            }
            .to_owned();
        }

        let description = match action_id.as_str() {
            "newtab" => "在当前窗口新建一个文档标签页",
            "newwindow" => "打开一个新的 gmark 窗口",
            "openfile" => "从本地选择并打开文档",
            "opensafesource" => "以安全的纯文本方式打开文档源文件",
            "openfolder" => "选择文件夹并在工作区中浏览",
            "openpreferences" => "打开外观、编辑器与快捷键设置",
            "savedocument" => "保存当前文档的全部更改",
            "savedocumentas" => "将当前文档保存到新的位置",
            "exporthtml" => "把当前文档导出为可在浏览器打开的 HTML 文件",
            "exportimage" => "把当前文档导出为 PNG 图片",
            "exportpdf" => "把当前文档导出为便于分享和打印的 PDF 文件",
            "exportselection" => "只导出当前选中的内容",
            "closedocument" | "closetab" => "关闭当前标签页；有未保存更改时会先询问",
            "closewindow" => "关闭当前窗口；有未保存更改时会先询问",
            "reopenclosedtab" => "恢复最近关闭的文档标签页",
            "previoustab" => "切换到左侧的标签页",
            "nexttab" => "切换到右侧的标签页",
            "quitapplication" => "退出 gmark；有未保存更改时会先询问",
            "undo" => "撤销最近一次编辑操作",
            "redo" => "恢复最近一次被撤销的编辑操作",
            "cut" => "剪切所选内容并放入剪贴板",
            "copy" => "复制所选内容到剪贴板",
            "copyasmarkdown" => "将所选内容以 Markdown 源文本复制",
            "paste" => "粘贴剪贴板内容并保留可识别格式",
            "pasteasplaintext" => "仅粘贴剪贴板中的纯文本",
            "selectall" => "选中当前文档中的全部内容",
            "findindocument" => "在当前文档中查找文字",
            "replaceindocument" => "查找并替换当前文档中的文字",
            "findnext" => "跳转到下一个查找结果",
            "findprevious" => "跳转到上一个查找结果",
            "quickopen" => "按文件名快速查找并打开工作区文件",
            "commandpalette" => "搜索并运行 gmark 提供的全部命令",
            "gotoline" => "按行号或字节位置跳转到文档位置",
            "toggleviewmode" => "在渲染视图与 Markdown 源码视图之间切换",
            "toggleworkspace" => "显示或隐藏工作区侧边栏",
            "togglefocusmode" => "隐藏非必要界面，让注意力集中在正文",
            "toggletypewritermode" => "输入时让当前行尽量保持在视口中央",
            "normalizelineendingslf" => "将整篇文档的换行符转换为 Unix/macOS 常用的 LF",
            "normalizelineendingscrlf" => "将整篇文档的换行符转换为 Windows 常用的 CRLF",
            "normalizelineendingscr" => "将整篇文档的换行符转换为传统 Mac 使用的 CR",
            "addlanguageconfig" => "导入自定义语言包并用于界面显示",
            "addthemeconfig" => "导入自定义主题配置并应用到界面",
            "selectlanguage" => "选择 gmark 界面使用的语言",
            "selecttheme" => "选择适合当前环境的界面主题",
            "checkforupdates" => "检查 GitHub Releases 中是否有更新版本",
            "opencrashreports" => "打开本机崩溃报告目录，便于排查问题",
            "openprivacypolicy" => "查看 gmark 的隐私政策",
            "showabout" => "查看 gmark 的版本、项目主页与许可信息",
            "installclitool" => "安装命令行工具，便于从终端打开文档",
            "uninstallclitool" => "从系统中移除 gmark 命令行工具",
            "pageup" => "向上滚动一页并移动光标",
            "pagedown" => "向下滚动一页并移动光标",
            "jumptotop" => "跳转到文档开头",
            "jumptobottom" => "跳转到文档末尾",
            "blockup" => "将光标移动到上一个内容块",
            "blockdown" => "将光标移动到下一个内容块",
            "moveleft" => "将光标向左移动一个字符",
            "moveright" => "将光标向右移动一个字符",
            "wordmoveleft" => "将光标移动到上一个单词",
            "wordmoveright" => "将光标移动到下一个单词",
            "home" => "将光标移动到当前行开头",
            "end" => "将光标移动到当前行末尾",
            "selectleft" => "向左扩展一个字符的选区",
            "selectright" => "向右扩展一个字符的选区",
            "wordselectleft" => "向左扩展一个单词的选区",
            "wordselectright" => "向右扩展一个单词的选区",
            "selecthome" => "将选区扩展到当前行开头",
            "selectend" => "将选区扩展到当前行末尾",
            "focusprev" => "将键盘焦点移动到上一个可操作区域",
            "focusnext" => "将键盘焦点移动到下一个可操作区域",
            "newline" => "在当前位置换行并继续输入",
            "deleteback" => "删除光标前的一个字符",
            "delete" => "删除光标后的一个字符",
            "worddeleteback" => "删除光标前的一个单词",
            "worddeleteforward" => "删除光标后的一个单词",
            "indentblock" => "增加当前块的缩进层级",
            "outdentblock" => "减少当前块的缩进层级",
            "exitcodeblock" => "离开当前代码块并继续输入正文",
            "dismisstransientui" => "关闭当前打开的菜单、面板或提示",
            _ => return format!("执行“{label}”命令"),
        };
        return description.to_owned();
    }

    if let Some(command) = editing_command_for_action(action) {
        return match command {
            EditingCommandId::BulletedList => "Convert the current paragraph to a bulleted list",
            EditingCommandId::NumberedList => "Convert the current paragraph to a numbered list",
            EditingCommandId::TaskList => "Convert the current paragraph to a task list",
            EditingCommandId::Quote => "Convert the current paragraph to a block quote",
            EditingCommandId::CodeBlock => {
                "Convert the current paragraph to a syntax-highlighted code block"
            }
            _ => return format!("Run the {label} command"),
        }
        .to_owned();
    }
    format!("Run the {label} command")
}
