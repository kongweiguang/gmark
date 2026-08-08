// @author kongweiguang

use super::html_node_id_from_asset_key;
use crate::editor::render_asset_manager;
use gmark_markdown::HtmlNodeId;

#[test]
fn html_asset_key_keeps_node_identity_recoverable_after_path() {
    let key = render_asset_manager::AssetKey::new(
        "doc/7/C:\\notes\\#html-node-name\\diagram.png#html-node-42",
        "v1",
        128,
        96,
    );

    assert_eq!(html_node_id_from_asset_key(&key), Some(HtmlNodeId(42)));
}

#[test]
fn standalone_asset_keys_have_no_html_node_identity() {
    let key = render_asset_manager::AssetKey::new("doc/7/C:\\notes\\diagram.png", "v1", 128, 96);

    assert_eq!(html_node_id_from_asset_key(&key), None);
}
