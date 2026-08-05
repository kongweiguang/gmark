// @author kongweiguang

//! A pill-shaped toggle switch component with slide animation.

use std::time::Instant;

use gpui::{prelude::FluentBuilder, *};

use crate::ui::{
    motion::{MotionOrigin, MotionTokens, TransitionState},
    theme::{ThemeManager, workbench::SurfaceKind},
    visual_preferences::VisualPreferencesManager,
};

type ClickHandler = dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static;
type KeyHandler = dyn Fn(&KeyDownEvent, &mut Window, &mut App) + 'static;

/// A toggle switch that can be checked or unchecked.
#[derive(IntoElement)]
pub(crate) struct Switch {
    id: ElementId,
    debug_selector: Option<&'static str>,
    checked: bool,
    disabled: bool,
    focus_handle: Option<FocusHandle>,
    on_click: Option<Box<ClickHandler>>,
    on_key_down: Option<Box<KeyHandler>>,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            debug_selector: None,
            checked: false,
            disabled: false,
            focus_handle: None,
            on_click: None,
            on_key_down: None,
        }
    }

    /// Set the checked state.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Set the click handler.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    /// Expose a stable selector to GPUI visual tests without deriving it from localized text.
    pub fn debug_selector(mut self, debug_selector: &'static str) -> Self {
        self.debug_selector = Some(debug_selector);
        self
    }

    /// Attach the stable focus identity owned by the surrounding view.
    pub fn focus_handle(mut self, focus_handle: FocusHandle) -> Self {
        self.focus_handle = Some(focus_handle);
        self
    }

    /// Set keyboard activation handling for the focused switch.
    pub fn on_key_down(
        mut self,
        handler: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_key_down = Some(Box::new(handler));
        self
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.global::<ThemeManager>().current().clone();
        let c = &theme.colors;
        let workbench = &c.workbench;

        let checked = self.checked;
        let disabled = self.disabled;

        let thumb_color = if disabled {
            c.dialog_secondary_button_text
        } else if checked {
            c.dialog_primary_button_text
        } else {
            c.dialog_secondary_button_text
        };

        let target = if checked { 1.0 } else { 0.0 };
        let visual_preferences = cx
            .try_global::<VisualPreferencesManager>()
            .map(VisualPreferencesManager::current)
            .unwrap_or_default();
        let duration = MotionTokens::default()
            .resolved(visual_preferences.reduced_motion)
            .switch;
        let solid_material = workbench.material(SurfaceKind::Solid, visual_preferences);
        let track_color = if disabled {
            solid_material.background
        } else if checked {
            workbench.accent
        } else {
            workbench.control_surface
        };
        let track_hover = if checked {
            workbench.accent_hover
        } else {
            workbench.control_hover
        };
        let track_pressed = if checked {
            workbench.accent_pressed
        } else {
            workbench.control_pressed
        };
        let now = Instant::now();
        let toggle_state =
            window.use_keyed_state::<TransitionState>(self.id.clone(), cx, |_, _| {
                if checked {
                    TransitionState::visible()
                } else {
                    TransitionState::hidden()
                }
            });
        toggle_state.update(cx, |state, _| {
            state.sample(now);
            if (state.target() - target).abs() >= f32::EPSILON {
                state.retarget(target, now, duration, MotionOrigin::Programmatic);
            }
        });
        let transition = toggle_state.read(cx).clone();
        let origin = transition.value() * 16.0;
        let target_margin = target * 16.0;
        let needs_animation = transition.is_active() && !duration.is_zero();
        let generation = transition.generation();

        let thumb = div()
            .w(px(16.0))
            .h(px(16.0))
            .rounded(px(8.0))
            .bg(thumb_color)
            .map(|mut this| {
                if needs_animation {
                    this.with_animation(
                        ElementId::NamedInteger("switch-move".into(), generation),
                        Animation::new(duration).with_easing(ease_out_quint()),
                        move |mut this, delta| {
                            let margin = origin + (target_margin - origin) * delta;
                            this.style().margin.left =
                                Some(Length::Definite(DefiniteLength::from(px(margin))));
                            this
                        },
                    )
                    .into_any_element()
                } else {
                    this.style().margin.left =
                        Some(Length::Definite(DefiniteLength::from(px(target_margin))));
                    this.into_any_element()
                }
            });

        div()
            .id(self.id)
            .when_some(self.debug_selector, |this, selector| {
                this.debug_selector(move || selector.to_owned())
            })
            .w(px(36.0))
            .h(px(20.0))
            .px(px(1.0))
            .flex()
            .items_center()
            .rounded(px(10.0))
            .border(px(1.0))
            .border_color(hsla(0.0, 0.0, 0.0, 0.0))
            .bg(track_color)
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(track_hover))
                    .active(move |style| style.bg(track_pressed))
            })
            .when_some(self.focus_handle, |this, focus_handle| {
                this.tab_index(0)
                    .track_focus(&focus_handle)
                    .focus(|this| this.border_color(workbench.focus_ring))
            })
            .child(thumb)
            .when_some(self.on_click, |this, on_click| this.on_click(on_click))
            .when_some(self.on_key_down, |this, on_key_down| {
                this.on_key_down(on_key_down)
            })
    }
}
