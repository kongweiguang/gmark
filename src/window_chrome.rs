// @author kongweiguang

//! Shared window chrome helpers for themed client-side title bars.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Decorations, Hsla, MouseButton, Pixels,
    PlatformDisplay, SharedString, TextAlign, TitlebarOptions, Window, WindowBackgroundAppearance,
    WindowBounds, WindowControlArea, WindowDecorations, WindowOptions, div, img, point, px, rgba,
    size, svg,
};

use crate::app_identity::GMARK_APP_ID;
use crate::theme::{Theme, ThemeDimensions};

const TITLEBAR_MIN_HEIGHT: f32 = 38.0;
const TITLEBAR_BUTTON_WIDTH: f32 = 46.0;
const TITLEBAR_ICON_SIZE: f32 = 12.0;
const TITLEBAR_LEADING_ICON_SLOT: f32 = 24.0;
const TITLEBAR_LEADING_ICON_SIZE: f32 = 20.0;
const MAC_TRAFFIC_LIGHT_RESERVED_WIDTH: f32 = 84.0;
const TITLEBAR_CLOSE_ICON: &str = "icon/titlebar/chrome-close.svg";
const TITLEBAR_MAXIMIZE_ICON: &str = "icon/titlebar/chrome-maximize.svg";
const TITLEBAR_MINIMIZE_ICON: &str = "icon/titlebar/chrome-minimize.svg";
const TITLEBAR_RESTORE_ICON: &str = "icon/titlebar/chrome-restore.svg";

/// 中间省略能在紧凑 chrome 中同时保留文件名前缀和 `.md` / `.markdown` 后缀。
pub(crate) fn middle_ellipsis(text: &str, max_chars: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return text.to_owned();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".into();
    }

    let suffix_len = (max_chars - 1) / 2;
    let prefix_len = max_chars - 1 - suffix_len;
    let mut compact = chars[..prefix_len].iter().collect::<String>();
    compact.push('…');
    compact.extend(chars[chars.len() - suffix_len..].iter());
    compact
}

/// Selects whether gmark or the platform should render window controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TitlebarControlMode {
    NativeTrafficLights,
    AppControls,
}

/// Layout metadata shared by editor and preferences windows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CustomTitlebarLayout {
    pub(crate) height: f32,
    pub(crate) controls: TitlebarControlMode,
}

/// Chooses the drag mechanism for the platform titlebar implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TitlebarDragStrategy {
    PlatformHitTest,
    ExplicitMoveRequest,
}

pub(crate) fn titlebar_options_for_target_os(
    target_os: &str,
    title: SharedString,
) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title),
        appears_transparent: matches!(target_os, "macos" | "windows"),
        traffic_light_position: if target_os == "macos" {
            Some(point(px(14.0), px(10.0)))
        } else {
            None
        },
    }
}

pub(crate) fn window_decorations_for_target_os(target_os: &str) -> Option<WindowDecorations> {
    match target_os {
        "linux" | "freebsd" => Some(WindowDecorations::Client),
        _ => None,
    }
}

pub(crate) fn gmark_window_options_for_target_os(
    target_os: &str,
    title: SharedString,
    bounds: Bounds<Pixels>,
) -> WindowOptions {
    gmark_window_options_with_bounds_for_target_os(target_os, title, WindowBounds::Windowed(bounds))
}

pub(crate) fn gmark_window_options_with_bounds_for_target_os(
    target_os: &str,
    title: SharedString,
    bounds: WindowBounds,
) -> WindowOptions {
    WindowOptions {
        app_id: Some(GMARK_APP_ID.to_string()),
        window_bounds: Some(bounds),
        titlebar: Some(titlebar_options_for_target_os(target_os, title)),
        window_background: WindowBackgroundAppearance::Opaque,
        window_decorations: window_decorations_for_target_os(target_os),
        ..WindowOptions::default()
    }
}

pub(crate) fn gmark_window_options(title: SharedString, bounds: Bounds<Pixels>) -> WindowOptions {
    gmark_window_options_for_target_os(std::env::consts::OS, title, bounds)
}

pub(crate) fn gmark_window_options_with_bounds(
    title: SharedString,
    bounds: WindowBounds,
) -> WindowOptions {
    gmark_window_options_with_bounds_for_target_os(std::env::consts::OS, title, bounds)
}

pub(crate) fn restored_window_bounds(
    saved: &crate::config::workspace_session::WorkspaceSessionWindow,
    cx: &App,
) -> WindowBounds {
    let displays = cx.displays();
    let preferred = saved.display_uuid.and_then(|uuid| {
        displays
            .iter()
            .find(|display| display.uuid().ok() == Some(uuid))
            .cloned()
    });
    let saved_bounds = Bounds::new(
        point(px(saved.x), px(saved.y)),
        size(px(saved.width), px(saved.height)),
    );
    let display = preferred
        .or_else(|| display_with_largest_overlap(&displays, saved_bounds))
        .or_else(|| cx.primary_display());
    let bounds = display
        .map(|display| clamp_window_to_display(saved_bounds, display.bounds()))
        .unwrap_or(saved_bounds);
    match saved.state {
        crate::config::workspace_session::WorkspaceSessionWindowState::Windowed => {
            WindowBounds::Windowed(bounds)
        }
        crate::config::workspace_session::WorkspaceSessionWindowState::Maximized => {
            WindowBounds::Maximized(bounds)
        }
        crate::config::workspace_session::WorkspaceSessionWindowState::Fullscreen => {
            WindowBounds::Fullscreen(bounds)
        }
    }
}

fn display_with_largest_overlap(
    displays: &[std::rc::Rc<dyn PlatformDisplay>],
    window: Bounds<Pixels>,
) -> Option<std::rc::Rc<dyn PlatformDisplay>> {
    displays
        .iter()
        .filter_map(|display| {
            let bounds = display.bounds();
            let width =
                f32::from(window.right().min(bounds.right()) - window.left().max(bounds.left()))
                    .max(0.0);
            let height =
                f32::from(window.bottom().min(bounds.bottom()) - window.top().max(bounds.top()))
                    .max(0.0);
            let area = width * height;
            (area > 0.0).then(|| (area, display.clone()))
        })
        .max_by(|(left, _), (right, _)| left.total_cmp(right))
        .map(|(_, display)| display)
}

fn clamp_window_to_display(window: Bounds<Pixels>, display: Bounds<Pixels>) -> Bounds<Pixels> {
    let display_width = f32::from(display.size.width).max(1.0);
    let display_height = f32::from(display.size.height).max(1.0);
    let width = f32::from(window.size.width).clamp(720.0_f32.min(display_width), display_width);
    let height = f32::from(window.size.height).clamp(520.0_f32.min(display_height), display_height);
    let min_x = f32::from(display.left());
    let min_y = f32::from(display.top());
    let max_x = f32::from(display.right()) - width;
    let max_y = f32::from(display.bottom()) - height;
    Bounds::new(
        point(
            px(f32::from(window.origin.x).clamp(min_x, max_x)),
            px(f32::from(window.origin.y).clamp(min_y, max_y)),
        ),
        size(px(width), px(height)),
    )
}

pub(crate) fn custom_titlebar_layout_for_target_os(
    target_os: &str,
    decorations: Decorations,
    dimensions: &ThemeDimensions,
) -> Option<CustomTitlebarLayout> {
    let height = dimensions.menu_bar_height.max(TITLEBAR_MIN_HEIGHT);
    match target_os {
        "macos" => Some(CustomTitlebarLayout {
            height,
            controls: TitlebarControlMode::NativeTrafficLights,
        }),
        "windows" => Some(CustomTitlebarLayout {
            height,
            controls: TitlebarControlMode::AppControls,
        }),
        "linux" | "freebsd" if matches!(decorations, Decorations::Client { .. }) => {
            Some(CustomTitlebarLayout {
                height,
                controls: TitlebarControlMode::AppControls,
            })
        }
        _ => None,
    }
}

/// Windows/macOS use hit-test drag areas; Linux client decorations need an explicit move request.
pub(crate) fn titlebar_drag_strategy_for_target_os(
    target_os: &str,
    decorations: Decorations,
) -> TitlebarDragStrategy {
    match target_os {
        "linux" | "freebsd" if matches!(decorations, Decorations::Client { .. }) => {
            TitlebarDragStrategy::ExplicitMoveRequest
        }
        _ => TitlebarDragStrategy::PlatformHitTest,
    }
}

pub(crate) fn custom_titlebar_height_for_target_os(
    target_os: &str,
    decorations: Decorations,
    dimensions: &ThemeDimensions,
) -> f32 {
    custom_titlebar_layout_for_target_os(target_os, decorations, dimensions)
        .map(|layout| layout.height)
        .unwrap_or(0.0)
}

pub(crate) fn custom_titlebar_height(window: &Window, dimensions: &ThemeDimensions) -> f32 {
    if cfg!(target_os = "macos") && window.is_fullscreen() {
        return 0.0;
    }

    custom_titlebar_height_for_target_os(
        std::env::consts::OS,
        window.window_decorations(),
        dimensions,
    )
}

pub(crate) fn custom_titlebar_background(theme: &Theme) -> Hsla {
    theme.colors.chrome_background
}

pub(crate) fn custom_titlebar_icon_color(theme: &Theme) -> Hsla {
    if custom_titlebar_background(theme).l < 0.5 {
        Hsla::from(rgba(0xf4f4f5ff))
    } else {
        Hsla::from(rgba(0x18181bff))
    }
}

pub(crate) fn titlebar_maximize_icon(is_maximized: bool, is_fullscreen: bool) -> &'static str {
    if is_maximized || is_fullscreen {
        TITLEBAR_RESTORE_ICON
    } else {
        TITLEBAR_MAXIMIZE_ICON
    }
}

pub(crate) fn render_custom_titlebar<T: 'static>(
    id: &'static str,
    title: Option<SharedString>,
    leading_icon: Option<&'static str>,
    theme: &Theme,
    window: &Window,
    cx: &mut Context<T>,
    on_close: fn(&mut T, &ClickEvent, &mut Window, &mut Context<T>),
) -> Option<AnyElement> {
    if cfg!(target_os = "macos") && window.is_fullscreen() {
        return None;
    }

    let layout = custom_titlebar_layout_for_target_os(
        std::env::consts::OS,
        window.window_decorations(),
        &theme.dimensions,
    )?;
    let drag_strategy =
        titlebar_drag_strategy_for_target_os(std::env::consts::OS, window.window_decorations());
    let c = &theme.colors;
    let t = &theme.typography;
    let controls = window.window_controls();
    let icon_color = custom_titlebar_icon_color(theme);
    let entity = cx.entity().downgrade();

    let centered_title = matches!(layout.controls, TitlebarControlMode::NativeTrafficLights);
    let title_label = title.map(|title| {
        div()
            .w_full()
            .min_w(px(0.0))
            .truncate()
            .when(centered_title, |this| this.text_align(TextAlign::Center))
            .debug_selector(move || format!("{id}-title-label"))
            .text_size(px(theme.dimensions.menu_text_size))
            .font_weight(t.dialog_button_weight.to_font_weight())
            .text_color(c.dialog_secondary_button_text)
            .child(title)
    });
    let drag_title = div()
        .id("window-titlebar-drag-title")
        .h_full()
        .flex_1()
        .min_w(px(0.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .gap(px(7.0))
        .window_control_area(WindowControlArea::Drag)
        .children(leading_icon.filter(|_| !centered_title).map(|path| {
            div()
                .size(px(TITLEBAR_LEADING_ICON_SLOT))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                // 标题栏 leading asset 当前用于完整品牌图标，不能经过单色 SVG 蒙版。
                .child(
                    img(path)
                        .size(px(TITLEBAR_LEADING_ICON_SIZE))
                        .debug_selector(move || format!("{id}-leading-icon")),
                )
        }))
        .children(title_label);

    let drag_title = match drag_strategy {
        TitlebarDragStrategy::PlatformHitTest => drag_title,
        TitlebarDragStrategy::ExplicitMoveRequest => {
            drag_title.on_mouse_down(MouseButton::Left, |event, window, cx| {
                if event.click_count >= 2 {
                    window.zoom_window();
                } else {
                    window.start_window_move();
                }
                cx.stop_propagation();
            })
        }
    }
    .on_click(|event, window, _cx| {
        if event.is_right_click() {
            window.show_window_menu(event.position());
        }
    });

    let root = div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(layout.height))
        .occlude()
        .flex()
        .items_center()
        .bg(custom_titlebar_background(theme))
        .border_b(px(theme.dimensions.dialog_border_width))
        .border_color(c.dialog_border);

    let root = match layout.controls {
        TitlebarControlMode::NativeTrafficLights => root
            .child(div().w(px(MAC_TRAFFIC_LIGHT_RESERVED_WIDTH)).h_full())
            .child(drag_title)
            .child(div().w(px(MAC_TRAFFIC_LIGHT_RESERVED_WIDTH)).h_full()),
        TitlebarControlMode::AppControls => {
            let close_entity = entity.clone();
            let mut controls_row = div().h_full().flex().items_center().flex_shrink_0();

            if controls.minimize {
                controls_row = controls_row.child(
                    div()
                        .id("window-titlebar-minimize")
                        .w(px(TITLEBAR_BUTTON_WIDTH))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .window_control_area(WindowControlArea::Min)
                        .hover(|this| this.bg(c.chrome_hover))
                        .cursor_pointer()
                        .child(
                            svg()
                                .path(TITLEBAR_MINIMIZE_ICON)
                                .size(px(TITLEBAR_ICON_SIZE))
                                .text_color(icon_color),
                        )
                        .on_click(|event, window, _cx| {
                            if event.standard_click() {
                                window.minimize_window();
                            }
                        }),
                );
            }

            if controls.maximize {
                controls_row = controls_row.child(
                    div()
                        .id("window-titlebar-maximize")
                        .w(px(TITLEBAR_BUTTON_WIDTH))
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .window_control_area(WindowControlArea::Max)
                        .hover(|this| this.bg(c.chrome_hover))
                        .cursor_pointer()
                        .child(
                            svg()
                                .path(titlebar_maximize_icon(
                                    window.is_maximized(),
                                    window.is_fullscreen(),
                                ))
                                .size(px(TITLEBAR_ICON_SIZE))
                                .text_color(icon_color),
                        )
                        .on_click(|event, window, _cx| {
                            if event.standard_click() {
                                window.zoom_window();
                            }
                        }),
                );
            }

            controls_row = controls_row.child(
                div()
                    .id("window-titlebar-close")
                    .w(px(TITLEBAR_BUTTON_WIDTH))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .window_control_area(WindowControlArea::Close)
                    .hover(|this| this.bg(c.dialog_danger_button_bg))
                    .cursor_pointer()
                    .child(
                        svg()
                            .path(TITLEBAR_CLOSE_ICON)
                            .size(px(TITLEBAR_ICON_SIZE))
                            .text_color(icon_color),
                    )
                    .on_click(move |event, window, app| {
                        if event.standard_click() {
                            let _ = close_entity.update(app, |view, cx| {
                                on_close(view, event, window, cx);
                            });
                        }
                    }),
            );

            root.child(drag_title).child(controls_row)
        }
    };

    Some(root.into_any_element())
}

#[cfg(test)]
#[path = "../tests/unit/window_chrome.rs"]
mod tests;
