// @author kongweiguang

//! Embedded application assets registered with GPUI during bootstrap.

use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

pub(crate) struct GmarkAssets;

impl AssetSource for GmarkAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        match path {
            "icon/gmark-icon.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/gmark-icon.svg"
            )))),
            "icon/workspace/folder.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/workspace/folder.svg"
            )))),
            "icon/workspace/markdown.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/workspace/markdown.svg"
            )))),
            "icon/titlebar/chrome-close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/titlebar/chrome-close.svg"
            )))),
            "icon/editor/tab-pin.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/editor/tab-pin.svg"
            )))),
            "icon/ui/files.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/files.svg"
            )))),
            "icon/ui/file.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/file.svg"
            )))),
            "icon/ui/outline.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/outline.svg"
            )))),
            "icon/ui/search.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/search.svg"
            )))),
            "icon/ui/panel-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/panel-left.svg"
            )))),
            "icon/ui/panel-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/panel-right.svg"
            )))),
            "icon/ui/live.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/live.svg"
            )))),
            "icon/ui/source.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/source.svg"
            )))),
            "icon/ui/split.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/split.svg"
            )))),
            "icon/ui/preview.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/preview.svg"
            )))),
            "icon/ui/close.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/close.svg"
            )))),
            "icon/ui/chevron-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/chevron-right.svg"
            )))),
            "icon/ui/chevron-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/chevron-down.svg"
            )))),
            "icon/ui/more-horizontal.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/more-horizontal.svg"
            )))),
            "icon/ui/case-sensitive.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/case-sensitive.svg"
            )))),
            "icon/ui/whole-word.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/whole-word.svg"
            )))),
            "icon/ui/regex.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/regex.svg"
            )))),
            "icon/ui/chevron-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/chevron-up.svg"
            )))),
            "icon/ui/copy.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/copy.svg"
            )))),
            "icon/ui/check.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/check.svg"
            )))),
            "icon/ui/code.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/code.svg"
            )))),
            "icon/ui/link.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/link.svg"
            )))),
            "icon/ui/palette.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/palette.svg"
            )))),
            "icon/ui/image.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/image.svg"
            )))),
            "icon/ui/expand.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/expand.svg"
            )))),
            "icon/ui/keyboard.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/keyboard.svg"
            )))),
            "icon/ui/panel-bottom.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/panel-bottom.svg"
            )))),
            "icon/ui/plus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/plus.svg"
            )))),
            "icon/ui/minus.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/minus.svg"
            )))),
            "icon/ui/type.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/type.svg"
            )))),
            "icon/ui/sun.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/sun.svg"
            )))),
            "icon/ui/moon.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/moon.svg"
            )))),
            "icon/ui/monitor.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/monitor.svg"
            )))),
            "icon/ui/save.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/save.svg"
            )))),
            "icon/ui/sliders.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/sliders.svg"
            )))),
            "icon/ui/undo.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/undo.svg"
            )))),
            "icon/ui/redo.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/redo.svg"
            )))),
            "icon/ui/scissors.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/scissors.svg"
            )))),
            "icon/ui/clipboard.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/clipboard.svg"
            )))),
            "icon/ui/power.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/power.svg"
            )))),
            "icon/ui/file-output.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/file-output.svg"
            )))),
            "icon/ui/refresh.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/refresh.svg"
            )))),
            "icon/ui/shield.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/shield.svg"
            )))),
            "icon/ui/info.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/info.svg"
            )))),
            "icon/ui/lightbulb.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/lightbulb.svg"
            )))),
            "icon/ui/triangle-alert.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/triangle-alert.svg"
            )))),
            "icon/ui/shield-alert.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/shield-alert.svg"
            )))),
            "icon/ui/heading-1.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/heading-1.svg"
            )))),
            "icon/ui/heading-2.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/heading-2.svg"
            )))),
            "icon/ui/heading-3.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/heading-3.svg"
            )))),
            "icon/ui/list.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/list.svg"
            )))),
            "icon/ui/list-ordered.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/list-ordered.svg"
            )))),
            "icon/ui/list-checks.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/list-checks.svg"
            )))),
            "icon/ui/quote.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/quote.svg"
            )))),
            "icon/ui/sigma.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/sigma.svg"
            )))),
            "icon/ui/corner-up-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/corner-up-left.svg"
            )))),
            "icon/ui/align-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/align-left.svg"
            )))),
            "icon/ui/align-center.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/align-center.svg"
            )))),
            "icon/ui/align-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/align-right.svg"
            )))),
            "icon/ui/arrow-left.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/arrow-left.svg"
            )))),
            "icon/ui/arrow-right.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/arrow-right.svg"
            )))),
            "icon/ui/arrow-up.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/arrow-up.svg"
            )))),
            "icon/ui/arrow-down.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/arrow-down.svg"
            )))),
            "icon/ui/trash.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/trash.svg"
            )))),
            "icon/ui/table.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/ui/table.svg"
            )))),
            "icon/titlebar/chrome-minimize.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/titlebar/chrome-minimize.svg"
            )))),
            "icon/titlebar/chrome-maximize.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/titlebar/chrome-maximize.svg"
            )))),
            "icon/titlebar/chrome-restore.svg" => Ok(Some(Cow::Borrowed(include_bytes!(
                "../../../assets/icon/titlebar/chrome-restore.svg"
            )))),
            _ => Ok(None),
        }
    }

    fn list(&self, _path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/app_assets.rs"]
mod tests;
