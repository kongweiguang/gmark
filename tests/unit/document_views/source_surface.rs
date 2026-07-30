// @author kongweiguang

use super::source_word_range;

#[test]
fn source_word_selection_keeps_unicode_and_emoji_boundaries() {
    assert_eq!(source_word_range("alpha 世界 🙂", 8), 6..12);
    assert_eq!(source_word_range("alpha 世界 🙂", 13), 13..17);
}
