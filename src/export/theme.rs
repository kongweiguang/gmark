// @author kongweiguang

//! Converts GPUI-owned theme values at the main-package boundary only.

use gpui::{Hsla, Rgba};

use crate::theme::{FontWeightDef, Theme};

pub(super) fn export_theme(theme: &Theme) -> gmark_export::ExportTheme {
    let colors = &theme.colors;
    let typography = &theme.typography;
    gmark_export::ExportTheme {
        color_scheme: if colors.editor_background.l < 0.5 {
            gmark_export::ExportColorScheme::Dark
        } else {
            gmark_export::ExportColorScheme::Light
        },
        colors: gmark_export::ExportThemeColors {
            background: color(colors.editor_background),
            text: color(colors.text_default),
            muted: color(colors.dialog_muted),
            link: color(colors.text_link),
            border: color(colors.table_border),
            code_background: color(colors.code_bg),
            code_text: color(colors.code_text),
            comment_background: color(colors.comment_bg),
            table_header_background: color(colors.table_header_bg),
            table_cell_background: color(colors.table_cell_bg),
            quote_border: color(colors.border_quote),
            quote_text: color(colors.text_quote),
            callout_note_background: color(colors.callout_note_bg),
            callout_note_border: color(colors.callout_note_border),
            callout_tip_background: color(colors.callout_tip_bg),
            callout_tip_border: color(colors.callout_tip_border),
            callout_important_background: color(colors.callout_important_bg),
            callout_important_border: color(colors.callout_important_border),
            callout_warning_background: color(colors.callout_warning_bg),
            callout_warning_border: color(colors.callout_warning_border),
            callout_caution_background: color(colors.callout_caution_bg),
            callout_caution_border: color(colors.callout_caution_border),
            heading: [
                color(colors.text_h1),
                color(colors.text_h2),
                color(colors.text_h3),
                color(colors.text_h4),
                color(colors.text_h5),
                color(colors.text_h6),
            ],
        },
        typography: gmark_export::ExportThemeTypography {
            text_size: typography.text_size,
            text_line_height: typography.text_line_height,
            heading_sizes: [
                typography.h1_size,
                typography.h2_size,
                typography.h3_size,
                typography.h4_size,
                typography.h5_size,
                typography.h6_size,
            ],
            heading_weight: font_weight(&typography.h1_weight),
            code_size: typography.code_size,
        },
        dimensions: gmark_export::ExportThemeDimensions {
            callout_radius: theme.dimensions.callout_radius,
            code_background_radius: theme.dimensions.code_bg_radius,
        },
    }
}

fn color(color: Hsla) -> gmark_export::ExportColor {
    let color = Rgba::from(color);
    gmark_export::ExportColor {
        red: channel(color.r),
        green: channel(color.g),
        blue: channel(color.b),
        alpha: color.a.clamp(0.0, 1.0),
    }
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn font_weight(weight: &FontWeightDef) -> gmark_export::ExportFontWeight {
    match weight {
        FontWeightDef::Thin => gmark_export::ExportFontWeight::Thin,
        FontWeightDef::Light => gmark_export::ExportFontWeight::Light,
        FontWeightDef::Normal => gmark_export::ExportFontWeight::Normal,
        FontWeightDef::Medium => gmark_export::ExportFontWeight::Medium,
        FontWeightDef::Semibold => gmark_export::ExportFontWeight::Semibold,
        FontWeightDef::Bold => gmark_export::ExportFontWeight::Bold,
        FontWeightDef::Extrabold => gmark_export::ExportFontWeight::Extrabold,
        FontWeightDef::Black => gmark_export::ExportFontWeight::Black,
    }
}
