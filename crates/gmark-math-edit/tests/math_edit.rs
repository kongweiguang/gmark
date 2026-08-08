// @author kongweiguang

use gmark_math_edit::{
    MATH_SYMBOL_PALETTE_KEYS, MathAst, MathCommand, MathCursor, MathCursor2D, MathDelimiterPair,
    MathDocument, MathEditCommand, MathEditor, MathNode, MathPath, MathSelection, MathSlot,
    MathSlotRole, MathSupportLevel,
};

#[test]
fn parses_known_constructs_without_changing_source() {
    let latex = r"x_1 + \frac{a^2}{\sqrt[3]{b}}";
    let document = MathDocument::parse(latex);
    assert_eq!(document.to_latex(), latex);
    assert!(matches!(
        document.ast().map(MathAst::root),
        Some(MathNode::Sequence(_))
    ));
    assert_eq!(document.support_level(), MathSupportLevel::Structured);
}

#[test]
fn malformed_and_unknown_input_round_trips() {
    let latex = r"\unknown{a} + \frac{unterminated";
    let document = MathDocument::parse(latex);
    assert_eq!(document.to_latex(), latex);
    assert!(document.to_latex().contains(r"\frac{unterminated"));
    assert_eq!(document.support_level(), MathSupportLevel::Opaque);
}

#[test]
fn matrix_environment_is_lossless_and_structured() {
    let source = r"\begin{matrix}a & b \\ c & d\end{matrix}";
    let document = MathDocument::parse(source);
    assert_eq!(document.to_latex(), source);
    assert_eq!(document.support_level(), MathSupportLevel::Structured);
    assert!(matches!(
        document.ast().map(MathAst::root),
        Some(MathNode::Environment { .. })
    ));
}

#[test]
fn known_environment_with_unknown_cell_construct_falls_back_to_source_editing() {
    let source = r"\begin{matrix}\custom{x} & b\\ c & d\end{matrix}";
    let document = MathDocument::parse(source);
    assert_eq!(document.to_latex(), source);
    assert_eq!(document.support_level(), MathSupportLevel::Opaque);
}

#[test]
fn opaque_documents_stay_opaque_after_source_edit() {
    let mut document = MathDocument::opaque(r"\custom{x}");
    document
        .replace_latex_range(8..9, "y")
        .expect("source edit");
    assert_eq!(document.to_latex(), r"\custom{y}");
    assert!(document.ast().is_none());
}

#[test]
fn structural_operations_address_sequence_children() {
    let mut ast = MathAst::parse("ab");
    ast.replace(&MathPath::root().child(0), MathNode::text("x"))
        .expect("replace");
    assert_eq!(ast.to_latex(), "x");
    ast.insert_after(&MathPath::root().child(0), MathNode::text("y"))
        .expect("insert");
    assert_eq!(ast.to_latex(), "xy");
}

#[test]
fn structural_selection_carries_node_range() {
    let ast = MathAst::parse(r"a+\frac{b}{c}");
    let path = MathPath::root().child(1);
    let selection = ast.select(&path).expect("selection");
    assert!(selection.is_structural());
    assert_eq!(
        selection.selected_text(&MathDocument::parse(ast.to_latex())),
        Some(r"\frac{b}{c}".into())
    );
}

#[test]
fn cursor_edits_on_unicode_character_boundaries() {
    let mut document = MathDocument::parse("α+β");
    let mut cursor = MathCursor::at(&document, "α+".len()).expect("cursor");
    assert!(cursor.delete_backward(&mut document).expect("delete"));
    cursor.insert(&mut document, "-").expect("insert");
    assert_eq!(document.to_latex(), "α-β");
    cursor.move_right(&document);
    assert_eq!(cursor.offset(), "α-β".len());
}

#[test]
fn command_templates_are_single_undoable_operations() {
    let mut editor = MathEditor::from_latex("x");
    let start = MathCursor2D::at(editor.document(), MathSlot::root(), 0).expect("start");
    let end = MathCursor2D::at(editor.document(), MathSlot::root(), 1).expect("end");
    editor
        .set_selection(MathSelection::new(start, end))
        .expect("selection");
    let result = editor
        .execute(MathEditCommand::InsertFraction)
        .expect("fraction");
    assert_eq!(result.before, "x");
    assert_eq!(result.after, r"\frac{x}{}");
    assert!(result.changed);
    assert!(editor.undo().expect("undo"));
    assert_eq!(editor.document().to_latex(), "x");
    assert!(editor.redo().expect("redo"));
    assert_eq!(editor.document().to_latex(), r"\frac{x}{}");
}

#[test]
fn all_structural_templates_preserve_selected_body() {
    let cases = [
        (MathEditCommand::InsertRoot, r"\sqrt{x}"),
        (MathEditCommand::InsertSuperscript, "x^{}"),
        (MathEditCommand::InsertSubscript, "x_{}"),
        (MathEditCommand::InsertTextMode, r"\text{x}"),
        (MathEditCommand::InsertSymbol("alpha".into()), r"\alpha"),
        (MathEditCommand::InsertBigOperator("sum".into()), r"\sum"),
        (MathEditCommand::InsertAccent("hat".into()), r"\hat{x}"),
        (
            MathEditCommand::InsertMatrix {
                rows: 2,
                columns: 2,
            },
            r"\begin{matrix}x &  \\  & \end{matrix}",
        ),
        (
            MathEditCommand::InsertCases { rows: 2 },
            r"\begin{cases}x &  \\  & \end{cases}",
        ),
        (
            MathEditCommand::InsertAligned {
                rows: 2,
                columns: 2,
            },
            r"\begin{aligned}x &  \\  & \end{aligned}",
        ),
    ];
    for (command, expected) in cases {
        let mut editor = MathEditor::from_latex("x");
        let start = MathCursor2D::at(editor.document(), MathSlot::root(), 0).expect("start");
        let end = MathCursor2D::at(editor.document(), MathSlot::root(), 1).expect("end");
        editor
            .set_selection(MathSelection::new(start, end))
            .expect("selection");
        assert_eq!(editor.execute(command).expect("command").after, expected);
    }
}

#[test]
fn two_dimensional_cursor_switches_fraction_slots() {
    let document = MathDocument::parse(r"\frac{abc}{xy}");
    let numerator = MathPath::root().child(0).child(0);
    let mut cursor = MathCursor2D::at(&document, numerator, 2).expect("numerator");
    assert!(cursor.move_down(&document).expect("down"));
    assert_eq!(cursor.path(), &MathPath::root().child(0).child(1));
    assert_eq!(cursor.offset(), 2);
    assert!(cursor.move_up(&document).expect("up"));
    assert_eq!(cursor.path(), &MathPath::root().child(0).child(0));
}

#[test]
fn two_dimensional_cursor_edits_root_without_double_offset() {
    let mut document = MathDocument::parse("abc");
    let mut cursor = MathCursor2D::at(&document, MathSlot::root(), 2).expect("cursor");
    cursor.delete_backward(&mut document).expect("backspace");
    assert_eq!(document.to_latex(), "ac");
    cursor.delete_forward(&mut document).expect("delete");
    assert_eq!(document.to_latex(), "a");
}

#[test]
fn two_dimensional_cursor_edits_nested_slot() {
    let mut document = MathDocument::parse(r"\frac{abc}{x}");
    let numerator = MathPath::root().child(0).child(0);
    let mut cursor = MathCursor2D::at(&document, numerator, 2).expect("cursor");
    cursor.delete_backward(&mut document).expect("backspace");
    assert_eq!(document.to_latex(), r"\frac{ac}{x}");
    cursor.insert(&mut document, "z").expect("insert");
    assert_eq!(document.to_latex(), r"\frac{azc}{x}");
}

#[test]
fn fraction_denominator_deletion_targets_content_not_the_open_brace() {
    let document = MathDocument::parse(r"\frac{a}{xyz}");
    let denominator = MathPath::root().child(0).child(1);
    let mut cursor = MathCursor2D::at(&document, denominator, 2).expect("denominator");
    let result = cursor
        .delete_backward(&mut document.clone())
        .expect("delete");
    assert_eq!(result.after, r"\frac{a}{xz}");
    assert_eq!(result.cursor.offset(), 1);
}

#[test]
fn environment_cells_are_navigable_and_editable() {
    let document = MathDocument::parse(r"\begin{matrix}a & b \\ c & d\end{matrix}");
    let ast = document.ast().expect("ast");
    let slots = ast.environment_slots(&MathPath::root());
    assert_eq!(slots.len(), 4);
    let mut cursor = MathCursor2D::at(&document, &slots[0], 1).expect("cell");
    assert!(cursor.move_down(&document).expect("down"));
    assert_eq!(
        cursor.slot().role(),
        gmark_math_edit::MathSlotRole::EnvironmentCell { row: 1, column: 0 }
    );
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    editor
        .execute(MathEditCommand::InsertText("z".into()))
        .expect("edit cell");
    assert!(editor.document().to_latex().contains('z'));
}

#[test]
fn deleting_at_environment_slot_edges_merges_cells_and_keeps_cursor_in_slot() {
    let document = MathDocument::parse(r"\begin{matrix}a & b \\ c & d\end{matrix}");
    let slots = document
        .ast()
        .expect("ast")
        .environment_slots(&MathPath::root());
    let cursor = MathCursor2D::at(&document, &slots[1], 0).expect("second cell start");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    let result = editor
        .execute(MathEditCommand::DeleteBackward)
        .expect("merge previous cell");
    assert!(result.changed);
    assert_eq!(
        editor.document().to_latex(),
        r"\begin{matrix}a b \\ c & d\end{matrix}"
    );
    assert_eq!(editor.cursor().slot().row(), Some(0));
    assert_eq!(editor.cursor().slot().column(), Some(0));
    assert_eq!(editor.cursor().offset(), 2);

    let cursor = MathCursor2D::at(editor.document(), editor.cursor().slot().clone(), 3)
        .expect("merged cell end");
    editor.set_cursor(cursor).expect("set cursor");
    let result = editor
        .execute(MathEditCommand::DeleteForward)
        .expect("merge next row cell");
    assert!(result.changed);
    assert_eq!(
        editor.document().to_latex(),
        r"\begin{matrix}a b c & d\end{matrix}"
    );
}

#[test]
fn tab_navigation_moves_between_environment_cells() {
    let document = MathDocument::parse(r"\begin{matrix}a & b \\ c & d\end{matrix}");
    let slots = document
        .ast()
        .expect("ast")
        .environment_slots(&MathPath::root());
    let mut cursor = MathCursor2D::at(&document, &slots[0], 1).expect("cell");
    assert!(cursor.move_environment_slot(&document, 1).expect("next"));
    assert_eq!(cursor.slot(), &slots[1]);
    assert!(
        cursor
            .move_environment_slot(&document, 1)
            .expect("next row")
    );
    assert_eq!(cursor.slot(), &slots[2]);
    assert!(
        cursor
            .move_environment_slot(&document, -1)
            .expect("previous")
    );
    assert_eq!(cursor.slot(), &slots[1]);
}

#[test]
fn known_and_unknown_constructs_round_trip_after_parse() {
    let samples = [
        r"\text{plain}",
        r"\hat{x}",
        r"\sum_{i=0}^{n} i",
        r"\begin{cases}x & x>0 \\ 0 & otherwise\end{cases}",
        r"\begin{custom}a & b\end{custom}",
        r"\unknown{a}[b]",
    ];
    for source in samples {
        let document = MathDocument::parse(source);
        assert_eq!(document.to_latex(), source, "{source}");
    }
}

#[test]
fn empty_and_unknown_environments_remain_lossless() {
    for source in [r"\begin{matrix}\end{matrix}", r"\begin{custom}\end{custom}"] {
        let document = MathDocument::parse(source);
        assert_eq!(document.to_latex(), source);
        let ast = document.ast().expect("ast");
        assert_eq!(ast.environment_slots(&MathPath::root()).len(), 1);
    }
}

#[test]
fn command_alias_remains_source_compatible() {
    let mut document = MathDocument::parse("x");
    let result = document
        .apply_command(MathCommand::InsertRoot)
        .expect("root");
    assert_eq!(result.after, r"\sqrt{}x");
    assert_eq!(document.to_latex(), r"\sqrt{}x");
}

#[test]
fn parser_round_trip_property_for_generated_fragments() {
    let atoms = ["a", "β", "+", "\\unknown", "{", "}", "^", "_", "\\%"];
    for prefix in atoms {
        for suffix in atoms {
            let source = format!("{prefix}{suffix}");
            assert_eq!(
                MathDocument::parse(&source).to_latex(),
                source,
                "{source:?}"
            );
        }
    }
}

#[test]
fn delimiter_pairs_round_trip_and_expose_body_slot() {
    for pair in MathDelimiterPair::all() {
        let source = pair.wrap_body("x");
        let document = MathDocument::parse(&source);
        assert_eq!(document.to_latex(), source);
        let Some(MathNode::Sequence(children)) = document.ast().map(MathAst::root) else {
            panic!("sequence root")
        };
        assert!(
            matches!(children.first(), Some(MathNode::Delimited { pair: got, .. }) if *got == pair)
        );
        assert_eq!(document.ast().expect("ast").paths().len(), 4);
    }
}

#[test]
fn new_structural_commands_place_cursor_in_editable_slots() {
    let mut editor = MathEditor::from_latex("x");
    let start = MathCursor2D::at(editor.document(), MathSlot::root(), 0).expect("cursor");
    let end = MathCursor2D::at(editor.document(), MathSlot::root(), 1).expect("cursor");
    editor
        .set_selection(MathSelection::new(start, end))
        .expect("selection");
    editor
        .execute(MathEditCommand::InsertNthRoot)
        .expect("nth root");
    assert_eq!(editor.document().to_latex(), r"\sqrt[]{x}");
    assert_eq!(editor.cursor().offset(), 0);
    assert!(matches!(editor.cursor().path().last(), Some(0)));

    let mut editor = MathEditor::from_latex("x");
    editor
        .execute(MathEditCommand::InsertDelimiter(MathDelimiterPair::Angle))
        .expect("delimiter");
    assert_eq!(
        editor.document().to_latex(),
        r"\left\langle \right\rangle x"
    );

    let mut editor = MathEditor::from_latex("");
    editor
        .execute(MathEditCommand::InsertOperatorWithLimits("sum".into()))
        .expect("operator");
    assert_eq!(editor.document().to_latex(), r"\sum_{}^{}");
    assert_eq!(editor.cursor().offset(), 0);
}

#[test]
fn visual_projection_uses_render_only_square_placeholders_and_hit_testing() {
    let document = MathDocument::parse(r"\frac{}{}");
    let projection = gmark_math_edit::MathVisualProjection::new(&document);
    assert_eq!(projection.to_latex(), r"\frac{}{}");
    assert!(projection.render_latex().contains(r"\square"));
    let numerator = MathPath::root().child(0).child(0);
    let cursor = MathCursor2D::at(&document, numerator, 0).expect("numerator cursor");
    let caret = projection.caret_rect(&cursor).expect("caret");
    assert!(caret.h > 0.0);
    let hit = projection
        .hit_test(caret.x, caret.y + caret.h / 2.0)
        .expect("hit");
    assert_eq!(hit.slot, *cursor.slot());
    let selection = document
        .ast()
        .expect("ast")
        .select(&MathPath::root().child(0))
        .expect("selection");
    assert!(projection.selection_rect(&selection).is_some());
}

#[test]
fn tab_slot_traversal_visits_fraction_and_radical_slots_in_order() {
    let document = MathDocument::parse(r"\frac{}{}+\sqrt[]{}".to_owned());
    let mut cursor = MathCursor2D::start(&document);
    let mut roles = Vec::new();
    while cursor.move_slot(&document, 1).expect("slot traversal") {
        roles.push(cursor.slot().role());
    }
    assert!(roles.iter().any(|role| matches!(role, MathSlotRole::Node)));
    assert!(
        roles.len() >= 4,
        "expected fraction and radical editable slots"
    );
}

#[test]
fn symbol_palette_keeps_the_reference_five_by_eight_order() {
    assert_eq!(MATH_SYMBOL_PALETTE_KEYS.len(), 40);
    assert_eq!(
        MATH_SYMBOL_PALETTE_KEYS,
        [
            "fraction",
            "sqrt",
            "nth_root",
            "matrix",
            "paren",
            "bracket",
            "brace",
            "abs",
            "norm",
            "angle",
            "floor",
            "ceil",
            "integral",
            "sum",
            "product",
            "infinity",
            "pi",
            "theta",
            "alpha",
            "beta",
            "gamma",
            "delta",
            "lambda",
            "mu",
            "sigma",
            "phi",
            "omega",
            "uppercase_delta",
            "less_or_equal",
            "greater_or_equal",
            "not_equal",
            "approximately",
            "times",
            "divide",
            "dot",
            "plus_minus",
            "right_arrow",
            "partial",
            "nabla",
            "member",
        ]
    );
}

#[test]
fn structural_commands_work_inside_nested_slots() {
    let document = MathDocument::parse(r"\frac{ab}{c}");
    let numerator = MathPath::root().child(0).child(0);
    let cursor = MathCursor2D::at(&document, numerator.clone(), 1).expect("cursor");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    editor
        .execute(MathEditCommand::InsertNthRoot)
        .expect("nested nth root");
    assert_eq!(editor.document().to_latex(), r"\frac{a\sqrt[]{}b}{c}");
    assert_eq!(editor.cursor().offset(), 0);
    assert!(editor.cursor().path().indices().len() >= 4);

    let document = MathDocument::parse(r"\frac{ab}{c}");
    let cursor = MathCursor2D::at(&document, numerator, 1).expect("cursor");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    editor
        .execute(MathEditCommand::InsertOperatorWithLimits("sum".into()))
        .expect("nested operator");
    assert_eq!(editor.document().to_latex(), r"\frac{a\sum_{}^{}b}{c}");
    assert_eq!(editor.cursor().offset(), 0);
}

#[test]
fn nested_environment_separators_remain_in_outer_cell() {
    let source = r"\begin{matrix}\begin{matrix}a & b\\c & d\end{matrix} & z\\q & r\end{matrix}";
    let document = MathDocument::parse(source);
    let slots = document
        .ast()
        .expect("ast")
        .environment_slots(&MathPath::root());
    assert_eq!(slots.len(), 4);
    assert_eq!(slots[0].row(), Some(0));
    assert_eq!(slots[0].column(), Some(0));
    let cursor = MathCursor2D::at(&document, &slots[0], 0).expect("nested slot");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    editor
        .execute(MathEditCommand::InsertText("x".into()))
        .expect("edit outer cell containing nested environment");
    assert!(
        editor
            .document()
            .to_latex()
            .contains(r"\begin{matrix}x\begin{matrix}"),
        "{}",
        editor.document().to_latex()
    );
}

#[test]
fn calculus_and_set_symbols_are_structured() {
    let source = r"\Delta + \div + \partial + \nabla + \in + \left\lfloor x \right\rfloor";
    let document = MathDocument::parse(source);
    assert_eq!(document.to_latex(), source);
    assert_eq!(document.support_level(), MathSupportLevel::Structured);
}

#[test]
fn deletion_handles_reversed_unicode_selection() {
    let mut editor = MathEditor::from_latex("aβc");
    let anchor =
        MathCursor2D::at(editor.document(), MathSlot::root(), "aβc".len()).expect("anchor");
    let focus = MathCursor2D::at(editor.document(), MathSlot::root(), 1).expect("focus");
    editor
        .set_selection(MathSelection::new(anchor, focus))
        .expect("selection");
    let result = editor
        .execute(MathEditCommand::DeleteBackward)
        .expect("delete selection");
    assert_eq!(result.after, "a");
    assert_eq!(editor.cursor().offset(), 1);
}

#[test]
fn delimiter_body_cursor_and_delete_use_the_real_separator_offset() {
    let source = r"\left\langle x+y \right\rangle";
    let mut document = MathDocument::parse(source);
    let body = MathPath::root().child(0).child(0);
    let mut cursor = MathCursor2D::at(&document, body, 1).expect("body start");
    let result = cursor.delete_forward(&mut document).expect("delete");
    assert!(result.changed);
    assert!(result.after.contains("+y"), "{}", result.after);
    assert!(!result.after.contains("x+y"), "{}", result.after);
}

#[test]
fn empty_fraction_root_and_script_slots_delete_their_structure() {
    let document = MathDocument::parse(r"\frac{}{}+x");
    let numerator = MathPath::root().child(0).child(0);
    let cursor = MathCursor2D::at(&document, numerator, 0).expect("numerator");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    assert_eq!(
        editor
            .execute(MathEditCommand::DeleteBackward)
            .expect("delete fraction")
            .after,
        "+x"
    );

    let document = MathDocument::parse(r"\sqrt{}");
    let radicand = MathPath::root().child(0).child(0);
    let cursor = MathCursor2D::at(&document, radicand, 0).expect("radicand");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    assert_eq!(
        editor
            .execute(MathEditCommand::DeleteForward)
            .expect("delete root")
            .after,
        ""
    );

    let document = MathDocument::parse(r"x^{}");
    let script = MathPath::root().child(1).child(0).child(0);
    let cursor = MathCursor2D::at(&document, script, 0).expect("script");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    assert_eq!(
        editor
            .execute(MathEditCommand::DeleteBackward)
            .expect("delete script")
            .after,
        "x"
    );

    let document = MathDocument::parse(r"\left( \right)");
    let body = MathPath::root().child(0).child(0);
    let cursor = MathCursor2D::at(&document, body, 1).expect("delimiter body");
    let mut editor =
        MathEditor::with_state(document, cursor.clone(), MathSelection::collapsed(cursor));
    assert_eq!(
        editor
            .execute(MathEditCommand::DeleteBackward)
            .expect("delete delimiter")
            .after,
        ""
    );
}

#[test]
fn cross_slot_fraction_selection_clears_contents_and_keeps_braces() {
    let document = MathDocument::parse(r"\frac{abc}{xyz}");
    let numerator = MathPath::root().child(0).child(0);
    let denominator = MathPath::root().child(0).child(1);
    let anchor = MathCursor2D::at(&document, numerator, 1).expect("numerator");
    let focus = MathCursor2D::at(&document, denominator, 2).expect("denominator");
    let mut editor =
        MathEditor::with_state(document, anchor.clone(), MathSelection::new(anchor, focus));
    let result = editor
        .execute(MathEditCommand::DeleteBackward)
        .expect("delete cross-slot selection");
    assert_eq!(result.after, r"\frac{a}{z}");
    assert_eq!(editor.cursor().path(), &MathPath::root().child(0).child(0));
    assert_eq!(editor.cursor().offset(), 1);
}

#[test]
fn cross_slot_matrix_selection_preserves_separators() {
    let source = r"\begin{matrix}ab&cd&ef\end{matrix}";
    let document = MathDocument::parse(source);
    let slots = document
        .ast()
        .expect("ast")
        .environment_slots(&MathPath::root());
    let anchor = MathCursor2D::at(&document, &slots[0], 1).expect("first cell");
    let focus = MathCursor2D::at(&document, &slots[2], 1).expect("last cell");
    let mut editor =
        MathEditor::with_state(document, anchor.clone(), MathSelection::new(anchor, focus));
    let result = editor
        .execute(MathEditCommand::DeleteForward)
        .expect("delete cross-cell selection");
    assert_eq!(result.after, r"\begin{matrix}a&&f\end{matrix}");
    assert_eq!(editor.cursor().slot().row(), Some(0));
    assert_eq!(editor.cursor().slot().column(), Some(0));
}
