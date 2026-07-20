// @author kongweiguang

use gpui::AssetSource;

use super::GmarkAssets;

#[test]
fn local_ui_icons_are_registered_with_gpui() {
    let assets = GmarkAssets;
    for path in [
        "icon/gmark-icon.svg",
        "icon/ui/file.svg",
        "icon/ui/plus.svg",
        "icon/ui/minus.svg",
        "icon/ui/type.svg",
        "icon/ui/sun.svg",
        "icon/ui/moon.svg",
        "icon/ui/monitor.svg",
        "icon/ui/save.svg",
        "icon/ui/sliders.svg",
        "icon/ui/undo.svg",
        "icon/ui/redo.svg",
        "icon/ui/scissors.svg",
        "icon/ui/clipboard.svg",
        "icon/ui/power.svg",
        "icon/ui/file-output.svg",
        "icon/ui/refresh.svg",
        "icon/ui/shield.svg",
        "icon/ui/info.svg",
        "icon/ui/lightbulb.svg",
        "icon/ui/triangle-alert.svg",
        "icon/ui/shield-alert.svg",
        "icon/ui/heading-1.svg",
        "icon/ui/heading-2.svg",
        "icon/ui/heading-3.svg",
        "icon/ui/list.svg",
        "icon/ui/list-ordered.svg",
        "icon/ui/list-checks.svg",
        "icon/ui/quote.svg",
        "icon/ui/sigma.svg",
        "icon/ui/corner-up-left.svg",
        "icon/ui/align-left.svg",
        "icon/ui/align-center.svg",
        "icon/ui/align-right.svg",
        "icon/ui/arrow-left.svg",
        "icon/ui/arrow-right.svg",
        "icon/ui/arrow-up.svg",
        "icon/ui/arrow-down.svg",
        "icon/ui/trash.svg",
        "icon/ui/table.svg",
    ] {
        let bytes = assets
            .load(path)
            .expect("embedded icon lookup should succeed")
            .unwrap_or_else(|| panic!("{path} should be registered"));
        assert!(bytes.starts_with(b"<!-- @author kongweiguang -->"));
        assert!(bytes.windows(4).any(|window| window == b"<svg"));
    }
}
