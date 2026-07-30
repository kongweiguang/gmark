// @author kongweiguang

use super::*;

/// Wraps a rendered inline link run so the hand cursor only appears while the
/// Cmd/Ctrl follow modifier is held. Links in mixed inline-visual blocks (math,
/// scripts, inline images) render as plain divs, so this sets `PointingHand`
/// when its hitbox is hovered and the modifier is down, like `BlockTextElement`
/// does for normal text. The editor root repaints on follow-modifier toggles,
/// so the cursor re-evaluates without the pointer moving. Layout and painting
/// are delegated to the child.
pub(super) struct LinkFollowCursor {
    pub(super) child: AnyElement,
}

impl IntoElement for LinkFollowCursor {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for LinkFollowCursor {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.child.prepaint(window, cx);
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if hitbox.is_hovered(window) && window.modifiers().secondary() {
            // The editor root repaints on follow-modifier toggles, so the hand
            // cursor re-evaluates here even while the pointer stays still.
            window.set_cursor_style(CursorStyle::PointingHand, hitbox);
        }
        self.child.paint(window, cx);
    }
}
