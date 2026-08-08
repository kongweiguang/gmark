// @author kongweiguang

use super::super::*;

impl Editor {
    pub(super) fn sync_accessibility_bridge(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let actions = self
            .accessibility_bridge
            .as_ref()
            .map(crate::accessibility::AccessibilityBridge::take_actions)
            .unwrap_or_default();
        for request in actions {
            if request.target_node == crate::accessibility::MATH_INPUT_ID
                && request.action == accesskit::Action::Focus
            {
                self.focus_accessibility_math_input(window, cx);
                continue;
            }
            if request.target_node == crate::accessibility::MATH_INPUT_ID
                && request.action == accesskit::Action::ReplaceSelectedText
            {
                if let Some(accesskit::ActionData::Value(value)) = request.data {
                    self.dispatch_accessibility_math_command(
                        gmark_math_edit::MathEditCommand::InsertText(value.into()),
                        cx,
                    );
                }
                continue;
            }
            if let Some(index) = crate::accessibility::math_action_index(request.target_node)
                && request.action == accesskit::Action::Click
            {
                let key = self.accessibility_snapshot(cx).math.and_then(|math| {
                    let page = math.page;
                    math.controls
                        .into_iter()
                        .filter(move |control| control.page == page)
                        .nth(index)
                        .map(|control| control.key)
                });
                if let Some(key) = key
                    && let Some(command) = accessibility_math_command(&key)
                {
                    self.dispatch_accessibility_math_command(command, cx);
                }
                continue;
            }
            if let Some((row, column)) =
                crate::accessibility::math_grid_cell_for_node(request.target_node)
                && matches!(
                    request.action,
                    accesskit::Action::Focus
                        | accesskit::Action::Click
                        | accesskit::Action::SetSequentialFocusNavigationStartingPoint
                )
            {
                self.focus_accessibility_math_grid_cell(row, column, cx);
                continue;
            }
            if request.action != accesskit::Action::Click {
                continue;
            }
            match request.target_node {
                crate::accessibility::MATH_SYMBOLS_TAB_ID => self
                    .set_accessibility_math_page(crate::components::MathPalettePage::Symbols, cx),
                crate::accessibility::MATH_STRUCTURES_TAB_ID => self.set_accessibility_math_page(
                    crate::components::MathPalettePage::Structures,
                    cx,
                ),
                crate::accessibility::MATH_PAGE_ID => cx.notify(),
                crate::accessibility::SAVE_ID => {
                    self.on_save_document(&crate::components::SaveDocument, window, cx)
                }
                crate::accessibility::FIND_ID => {
                    self.on_find_in_document_action(&crate::components::FindInDocument, window, cx)
                }
                crate::accessibility::GO_TO_LINE_ID => {
                    self.on_go_to_line_action(&crate::components::GoToLine, window, cx)
                }
                crate::accessibility::ERROR_ID => {
                    if let Some(document_host) = self.document_host.clone() {
                        document_host.update(cx, |view, cx| view.activate_accessibility_error(cx));
                    }
                }
                target => {
                    if let Some(line) = crate::accessibility::source_line_for_fold_node(target) {
                        if let Some(document_host) = self.document_host.clone() {
                            document_host
                                .update(cx, |view, cx| view.toggle_fold_at_source_line(line, cx));
                        } else {
                            let fold_action = self
                                .accessibility_snapshot(cx)
                                .folds
                                .into_iter()
                                .find_map(|fold| {
                                    (fold.start_line == line as u64).then_some(fold.target)
                                })
                                .flatten();
                            if let Some(crate::accessibility::AccessibilityFoldTarget::Rendered {
                                key,
                                heading,
                            }) = fold_action
                            {
                                self.toggle_rendered_collapse(&key, heading, cx);
                            }
                        }
                    }
                }
            }
        }
        if let Some(bridge) = self.accessibility_bridge.as_mut() {
            bridge.update_focus(window.is_window_active());
        }
        let revision = self.current_accessibility_revision(cx);
        if self.accessibility_revision != Some(revision) {
            let snapshot = self.accessibility_snapshot(cx);
            if let Some(bridge) = self.accessibility_bridge.as_mut() {
                bridge.update(snapshot);
            }
            self.accessibility_revision = Some(revision);
        }
    }

    fn dispatch_accessibility_math_command(
        &mut self,
        command: gmark_math_edit::MathEditCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(entity_id) = self.active_entity_id.or(self.pending_focus) else {
            return;
        };
        let Some(block) = self.focusable_entity_by_id(entity_id) else {
            return;
        };
        block.update(cx, |block, cx| {
            if block.math_edit_session.is_some() {
                let _ = block.execute_math_palette_command(command, cx);
            }
        });
    }

    fn focus_accessibility_math_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entity_id) = self.active_entity_id.or(self.pending_focus) else {
            return;
        };
        let Some(block) = self.focusable_entity_by_id(entity_id) else {
            return;
        };
        block.update(cx, |block, _cx| {
            block.math_structure_focus_handle.focus(window);
        });
    }

    fn set_accessibility_math_page(
        &mut self,
        page: crate::components::MathPalettePage,
        cx: &mut Context<Self>,
    ) {
        let Some(entity_id) = self.active_entity_id.or(self.pending_focus) else {
            return;
        };
        let Some(block) = self.focusable_entity_by_id(entity_id) else {
            return;
        };
        block.update(cx, |block, cx| {
            block.math_palette_page = page;
            cx.notify();
        });
    }

    fn focus_accessibility_math_grid_cell(
        &mut self,
        row: usize,
        column: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(entity_id) = self.active_entity_id.or(self.pending_focus) else {
            return;
        };
        let Some(block) = self.focusable_entity_by_id(entity_id) else {
            return;
        };
        block.update(cx, |block, cx| {
            let Some(session) = block.math_edit_session.as_mut() else {
                return;
            };
            let current = session.editor().cursor().clone();
            let slot = gmark_math_edit::MathSlot::environment_cell(
                current.slot().path().clone(),
                row,
                column,
            );
            let Ok(cursor) =
                gmark_math_edit::MathCursor2D::at(session.document(), slot, current.offset())
            else {
                return;
            };
            if session.editor_mut().set_cursor(cursor).is_ok() {
                cx.notify();
            }
        });
    }
}

fn accessibility_math_command(key: &str) -> Option<gmark_math_edit::MathEditCommand> {
    use gmark_math_edit::{MathDelimiterPair, MathEditCommand};

    Some(match key {
        "fraction" => MathEditCommand::InsertFraction,
        "sqrt" => MathEditCommand::InsertSquareRoot,
        "nth_root" => MathEditCommand::InsertNthRoot,
        "paren" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Parentheses),
        "bracket" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Brackets),
        "brace" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Braces),
        "abs" => MathEditCommand::InsertDelimiter(MathDelimiterPair::AbsoluteValue),
        "norm" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Norm),
        "angle" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Angle),
        "floor" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Floor),
        "ceil" => MathEditCommand::InsertDelimiter(MathDelimiterPair::Ceil),
        "integral" => MathEditCommand::InsertOperatorWithLimits("int".to_owned()),
        "sum" => MathEditCommand::InsertOperatorWithLimits("sum".to_owned()),
        "product" => MathEditCommand::InsertOperatorWithLimits("prod".to_owned()),
        "infinity" => MathEditCommand::InsertText(r"\infty".to_owned()),
        "pi" => MathEditCommand::InsertText(r"\pi".to_owned()),
        "theta" => MathEditCommand::InsertText(r"\theta".to_owned()),
        "alpha" => MathEditCommand::InsertText(r"\alpha".to_owned()),
        "beta" => MathEditCommand::InsertText(r"\beta".to_owned()),
        "gamma" => MathEditCommand::InsertText(r"\gamma".to_owned()),
        "delta" => MathEditCommand::InsertText(r"\delta".to_owned()),
        "lambda" => MathEditCommand::InsertText(r"\lambda".to_owned()),
        "mu" => MathEditCommand::InsertText(r"\mu".to_owned()),
        "sigma" => MathEditCommand::InsertText(r"\sigma".to_owned()),
        "phi" => MathEditCommand::InsertText(r"\phi".to_owned()),
        "omega" => MathEditCommand::InsertText(r"\omega".to_owned()),
        "uppercase_delta" => MathEditCommand::InsertText(r"\Delta".to_owned()),
        "less_or_equal" => MathEditCommand::InsertText(r"\le".to_owned()),
        "greater_or_equal" => MathEditCommand::InsertText(r"\ge".to_owned()),
        "not_equal" => MathEditCommand::InsertText(r"\ne".to_owned()),
        "approximately" => MathEditCommand::InsertText(r"\approx".to_owned()),
        "times" => MathEditCommand::InsertText(r"\times".to_owned()),
        "divide" => MathEditCommand::InsertText(r"\div".to_owned()),
        "dot" => MathEditCommand::InsertText(r"\cdot".to_owned()),
        "plus_minus" => MathEditCommand::InsertText(r"\pm".to_owned()),
        "right_arrow" => MathEditCommand::InsertText(r"\to".to_owned()),
        "partial" => MathEditCommand::InsertText(r"\partial".to_owned()),
        "nabla" => MathEditCommand::InsertText(r"\nabla".to_owned()),
        "member" => MathEditCommand::InsertText(r"\in".to_owned()),
        "matrix" => MathEditCommand::InsertMatrix {
            rows: 2,
            columns: 2,
        },
        "superscript" => MathEditCommand::InsertSuperscript,
        "subscript" => MathEditCommand::InsertSubscript,
        "cases" => MathEditCommand::InsertCases { rows: 2 },
        "aligned" => MathEditCommand::InsertAligned {
            rows: 2,
            columns: 2,
        },
        "text_mode" => MathEditCommand::InsertTextMode,
        _ => return None,
    })
}
