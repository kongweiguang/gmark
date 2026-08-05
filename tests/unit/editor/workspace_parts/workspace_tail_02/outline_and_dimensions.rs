// @author kongweiguang

    #[test]
    fn outline_tree_skips_headings_inside_fenced_code() {
        let outline = build_outline_tree(
            "# Root\n\n```md\n# ignored\n```\n\n## Child\n\n### Grandchild\n\n# Next",
        );

        assert_eq!(outline.len(), 2);
        assert_eq!(outline[0].label, "Root");
        assert_eq!(outline[0].children[0].label, "Child");
        assert_eq!(outline[0].children[0].children[0].label, "Grandchild");
        assert_eq!(outline[1].label, "Next");
    }

    #[test]
    fn outline_expansion_state_is_not_auto_populated_and_prunes_stale_ids() {
        let outline = build_outline_tree("# Root\n\n## Child\n\n# Next");
        let mut fresh = WorkspaceState::default();
        prune_outline_state(&mut fresh, &outline);
        assert!(fresh.expanded.is_empty());

        let mut existing = WorkspaceState::default();
        existing.expanded.insert("outline:0".to_string());
        existing.expanded.insert("outline:999".to_string());
        existing
            .expanded
            .insert("workspace-dir:C:/docs".to_string());
        existing.selected = Some(WorkspaceSelection::Outline("outline:999".to_string()));

        prune_outline_state(&mut existing, &outline);

        assert!(existing.expanded.contains("outline:0"));
        assert!(existing.expanded.contains("workspace-dir:C:/docs"));
        assert!(!existing.expanded.contains("outline:999"));
        assert_eq!(existing.selected, None);
    }

    #[test]
    fn workspace_panel_width_uses_ratio_with_bounds() {
        assert!(workspace_uses_overlay(899.0));
        assert!(!workspace_uses_overlay(900.0));
        assert_eq!(
            workspace_panel_width_for_viewport(899.0, None),
            WORKSPACE_COMPACT_OVERLAY_WIDTH
        );
        assert_eq!(
            workspace_panel_width_for_viewport(900.0, None),
            WORKSPACE_PANEL_AUTO_MIN_WIDTH
        );
        assert_eq!(workspace_panel_width_for_viewport(720.0, None), 280.0);
        assert_eq!(workspace_panel_width_for_viewport(1000.0, None), 248.0);
        assert_eq!(workspace_panel_width_for_viewport(2000.0, None), 300.0);
        assert_eq!(workspace_panel_width_for_viewport(4000.0, None), 360.0);
        assert_eq!(
            workspace_panel_width_for_viewport(1000.0, Some(200.0)),
            200.0
        );
        assert_eq!(
            workspace_panel_width_for_viewport(1000.0, Some(320.0)),
            320.0
        );
        assert_eq!(
            workspace_panel_width_for_viewport(1000.0, Some(900.0)),
            360.0
        );
        assert_eq!(
            workspace_panel_width_for_viewport(720.0, Some(320.0)),
            280.0
        );
    }

    #[test]
    fn document_sidebar_width_uses_independent_bounds_and_compact_overlay() {
        assert_eq!(
            document_sidebar_panel_width_for_viewport(899.0, None),
            DOCUMENT_SIDEBAR_COMPACT_OVERLAY_WIDTH
        );
        assert_eq!(
            document_sidebar_panel_width_for_viewport(900.0, None),
            DOCUMENT_SIDEBAR_PANEL_AUTO_MIN_WIDTH
        );
        assert_eq!(
            document_sidebar_panel_width_for_viewport(1200.0, None),
            240.0
        );
        assert_eq!(
            document_sidebar_panel_width_for_viewport(1200.0, Some(180.0)),
            DOCUMENT_SIDEBAR_PANEL_MIN_WIDTH
        );
        assert_eq!(
            document_sidebar_panel_width_for_viewport(1200.0, Some(480.0)),
            DOCUMENT_SIDEBAR_PANEL_MAX_WIDTH
        );
        assert_eq!(
            document_sidebar_panel_width_for_viewport(720.0, Some(320.0)),
            DOCUMENT_SIDEBAR_COMPACT_OVERLAY_WIDTH
        );
    }
