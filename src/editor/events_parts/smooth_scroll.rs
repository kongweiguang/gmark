// @author kongweiguang

//! 鼠标行级滚轮的单一合并动画；像素级触控板输入始终保留平台原生行为。

use super::*;

const SMOOTH_SCROLL_DURATION: Duration = Duration::from_millis(140);
const SMOOTH_SCROLL_FRAME_INTERVAL: Duration = Duration::from_millis(8);
const SMOOTH_SCROLL_EXTERNAL_OFFSET_EPSILON: f32 = 0.5;

impl Editor {
    /// GPUI 先处理同一元素的 overflow，再调用自定义 wheel listener。行级滚轮在此
    /// 同一事件内撤销原生跳变并启动插值，因此不会把“先跳再回”的中间状态绘制出来。
    /// 像素级输入来自触控板等精确设备，保留平台原生位移并立即接管旧动画。
    pub(super) fn handle_scroll_wheel(
        &mut self,
        driver: SplitScrollDriver,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ScrollDelta::Lines(lines) = event.delta else {
            self.cancel_smooth_scroll();
            return;
        };
        let line_delta = if lines.y != 0.0 { lines.y } else { lines.x };
        if line_delta == 0.0 {
            return;
        }

        self.retarget_smooth_scroll(driver, window.line_height() * line_delta, cx);
    }

    fn retarget_smooth_scroll(
        &mut self,
        driver: SplitScrollDriver,
        delta_y: Pixels,
        cx: &mut Context<Self>,
    ) {
        let Some(scroll_handle) = self.scroll_handle_for_driver(driver) else {
            self.cancel_smooth_scroll();
            return;
        };
        let max_offset_y = scroll_handle.max_offset().height.max(px(0.0));
        // overflow_y_scroll 已经加过一次 delta；减回去后才是用户本帧看到的起点。
        let native_y = scroll_handle.offset().y;
        let restored_y = Self::clamp_vertical_scroll_offset(native_y - delta_y, max_offset_y);
        let previous_target = self
            .smooth_scroll_animation
            .filter(|animation| animation.driver == driver)
            .map(|animation| animation.target_y);
        let target_y =
            Self::smooth_scroll_target(restored_y, previous_target, delta_y, max_offset_y);

        let mut offset = scroll_handle.offset();
        offset.y = restored_y;
        scroll_handle.set_offset(offset);
        self.pending_scroll_active_block_into_view = false;
        self.pending_scroll_recheck_after_layout = false;

        if target_y == restored_y {
            self.cancel_smooth_scroll();
            return;
        }

        self.smooth_scroll_animation = Some(SmoothScrollAnimation {
            driver,
            start_y: restored_y,
            target_y,
            last_applied_y: restored_y,
            started_at: cx.background_executor().now(),
        });
        self.ensure_smooth_scroll_task(cx);
    }

    fn ensure_smooth_scroll_task(&mut self, cx: &mut Context<Self>) {
        if self.smooth_scroll_task.is_some() {
            return;
        }
        self.smooth_scroll_task = Some(cx.spawn(
            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| loop {
                cx.background_executor()
                    .timer(SMOOTH_SCROLL_FRAME_INTERVAL)
                    .await;
                let now = cx.background_executor().now();
                let keep_running = this
                    .update(cx, |this, cx| this.advance_smooth_scroll(now, cx))
                    .unwrap_or_default();
                if !keep_running {
                    break;
                }
            },
        ));
    }

    fn advance_smooth_scroll(&mut self, now: Instant, cx: &mut Context<Self>) -> bool {
        let Some(mut animation) = self.smooth_scroll_animation else {
            self.smooth_scroll_task = None;
            return false;
        };
        let Some(scroll_handle) = self.scroll_handle_for_driver(animation.driver) else {
            self.cancel_smooth_scroll();
            return false;
        };
        let actual_y = scroll_handle.offset().y;
        if f32::from(actual_y - animation.last_applied_y).abs()
            > SMOOTH_SCROLL_EXTERNAL_OFFSET_EPSILON
        {
            // 滚动条拖拽、跳转或布局恢复拥有更高优先级，动画不得把它拉回旧目标。
            self.cancel_smooth_scroll();
            return false;
        }

        let max_offset_y = scroll_handle.max_offset().height.max(px(0.0));
        animation.target_y = Self::clamp_vertical_scroll_offset(animation.target_y, max_offset_y);
        let progress =
            Self::smooth_scroll_progress(now.saturating_duration_since(animation.started_at));
        let start = f32::from(animation.start_y);
        let target = f32::from(animation.target_y);
        let next_y = px(start + (target - start) * progress);
        let next_y = Self::clamp_vertical_scroll_offset(next_y, max_offset_y);
        let mut offset = scroll_handle.offset();
        offset.y = next_y;
        scroll_handle.set_offset(offset);

        if let Some(state) = self.split_preview.as_mut() {
            state.scroll_driver = Some(animation.driver);
        }
        if progress >= 1.0 || next_y == animation.target_y {
            self.smooth_scroll_animation = None;
            self.smooth_scroll_task = None;
            cx.notify();
            false
        } else {
            animation.last_applied_y = next_y;
            self.smooth_scroll_animation = Some(animation);
            cx.notify();
            true
        }
    }

    fn scroll_handle_for_driver(&self, driver: SplitScrollDriver) -> Option<ScrollHandle> {
        match driver {
            SplitScrollDriver::Source => Some(self.scroll_handle.clone()),
            SplitScrollDriver::Preview => self
                .split_preview
                .as_ref()
                .map(|state| state.scroll_handle.clone()),
        }
    }

    pub(super) fn cancel_smooth_scroll(&mut self) {
        self.smooth_scroll_animation = None;
        self.smooth_scroll_task = None;
    }

    pub(super) fn clamp_vertical_scroll_offset(target_y: Pixels, max_offset_y: Pixels) -> Pixels {
        target_y.min(px(0.0)).max(-max_offset_y.max(px(0.0)))
    }

    fn smooth_scroll_target(
        current_y: Pixels,
        previous_target_y: Option<Pixels>,
        delta_y: Pixels,
        max_offset_y: Pixels,
    ) -> Pixels {
        Self::clamp_vertical_scroll_offset(
            previous_target_y.unwrap_or(current_y) + delta_y,
            max_offset_y,
        )
    }

    pub(in crate::editor) fn smooth_scroll_progress(elapsed: Duration) -> f32 {
        let progress =
            (elapsed.as_secs_f32() / SMOOTH_SCROLL_DURATION.as_secs_f32()).clamp(0.0, 1.0);
        1.0 - (1.0 - progress).powi(3)
    }
}
