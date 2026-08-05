// @author kongweiguang

//! Shared motion tokens and an interruptible scalar transition state.

use std::time::{Duration, Instant};

/// Stable motion durations for low-frequency workbench feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MotionTokens {
    pub feedback_enter: Duration,
    pub feedback_exit: Duration,
    pub switch: Duration,
    pub popover_enter: Duration,
    pub popover_exit: Duration,
    pub dialog_enter: Duration,
    pub dialog_exit: Duration,
    pub drawer_enter: Duration,
    pub drawer_exit: Duration,
    pub toast_enter: Duration,
    pub toast_exit: Duration,
    pub discrete_wheel: Duration,
    /// Reduced Motion may keep this short opacity-only affordance.
    pub reduced_opacity: Duration,
}

impl Default for MotionTokens {
    fn default() -> Self {
        Self {
            feedback_enter: Duration::from_millis(100),
            feedback_exit: Duration::from_millis(80),
            switch: Duration::from_millis(160),
            popover_enter: Duration::from_millis(150),
            popover_exit: Duration::from_millis(110),
            dialog_enter: Duration::from_millis(190),
            dialog_exit: Duration::from_millis(150),
            drawer_enter: Duration::from_millis(220),
            drawer_exit: Duration::from_millis(180),
            toast_enter: Duration::from_millis(160),
            toast_exit: Duration::from_millis(120),
            discrete_wheel: Duration::from_millis(140),
            reduced_opacity: Duration::from_millis(80),
        }
    }
}

impl MotionTokens {
    /// Disables movement-producing transitions while retaining the separate
    /// opacity-only allowance for surfaces that need visual continuity.
    #[must_use]
    pub(crate) fn resolved(mut self, reduced_motion: bool) -> Self {
        if reduced_motion {
            self.feedback_enter = Duration::ZERO;
            self.feedback_exit = Duration::ZERO;
            self.switch = Duration::ZERO;
            self.popover_enter = Duration::ZERO;
            self.popover_exit = Duration::ZERO;
            self.dialog_enter = Duration::ZERO;
            self.dialog_exit = Duration::ZERO;
            self.drawer_enter = Duration::ZERO;
            self.drawer_exit = Duration::ZERO;
            self.toast_enter = Duration::ZERO;
            self.toast_exit = Duration::ZERO;
            self.discrete_wheel = Duration::ZERO;
        }
        self
    }
}

/// Mount lifecycle of an animated transient surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionPhase {
    Hidden,
    Entering,
    Visible,
    Exiting,
}

/// Source of a motion request, useful for choosing restrained behavior.
// Reason: later UI slices need all origins; remove after TASK-003 through TASK-009 consume them.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MotionOrigin {
    Pointer,
    Keyboard,
    Programmatic,
    Gesture,
    ResponsiveResize,
}

/// Interruptible transition whose generation rejects stale completions.
#[derive(Clone, Debug)]
pub(crate) struct TransitionState {
    phase: MotionPhase,
    from: f32,
    value: f32,
    target: f32,
    velocity: f32,
    started_at: Instant,
    duration: Duration,
    generation: u64,
    origin: MotionOrigin,
}

impl TransitionState {
    #[must_use]
    pub(crate) fn hidden() -> Self {
        Self::settled(0.0, MotionPhase::Hidden)
    }

    #[must_use]
    pub(crate) fn visible() -> Self {
        Self::settled(1.0, MotionPhase::Visible)
    }

    fn settled(value: f32, phase: MotionPhase) -> Self {
        Self {
            phase,
            from: value,
            value,
            target: value,
            velocity: 0.0,
            started_at: Instant::now(),
            duration: Duration::ZERO,
            generation: 0,
            origin: MotionOrigin::Programmatic,
        }
    }

    // Reason: later transient surfaces inspect phase; remove after their owners land.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) const fn phase(&self) -> MotionPhase {
        self.phase
    }

    #[must_use]
    pub(crate) const fn value(&self) -> f32 {
        self.value
    }

    // Reason: later retargeting consumes velocity; remove after that motion consumer lands.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) const fn velocity(&self) -> f32 {
        self.velocity
    }

    #[must_use]
    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub(crate) const fn target(&self) -> f32 {
        self.target
    }

    // Reason: later surfaces consume origin policy; remove after TASK-003 through TASK-009 land.
    #[allow(dead_code)]
    #[must_use]
    pub(crate) const fn origin(&self) -> MotionOrigin {
        self.origin
    }

    #[must_use]
    pub(crate) fn is_active(&self) -> bool {
        matches!(self.phase, MotionPhase::Entering | MotionPhase::Exiting)
    }

    /// Retargets from the current presentation value, never the old logical target.
    pub(crate) fn retarget(
        &mut self,
        target: f32,
        now: Instant,
        duration: Duration,
        origin: MotionOrigin,
    ) -> u64 {
        self.sample(now);
        self.generation = self.generation.wrapping_add(1);
        self.from = self.value;
        self.target = target.clamp(0.0, 1.0);
        self.started_at = now;
        self.duration = duration;
        self.origin = origin;
        if duration.is_zero() || (self.from - self.target).abs() < f32::EPSILON {
            self.value = self.target;
            self.velocity = 0.0;
            self.phase = if self.target > 0.0 {
                MotionPhase::Visible
            } else {
                MotionPhase::Hidden
            };
        } else {
            self.phase = if self.target > self.from {
                MotionPhase::Entering
            } else {
                MotionPhase::Exiting
            };
        }
        self.generation
    }

    /// Samples a bounded native-style ease-out curve at `now`.
    pub(crate) fn sample(&mut self, now: Instant) -> f32 {
        if !self.is_active() {
            return self.value;
        }
        let elapsed = now.saturating_duration_since(self.started_at);
        if elapsed >= self.duration {
            self.finish();
            return self.value;
        }
        let t = elapsed.as_secs_f32() / self.duration.as_secs_f32();
        let remaining = 1.0 - t.clamp(0.0, 1.0);
        let eased = 1.0 - remaining.powi(5);
        self.value = self.from + (self.target - self.from) * eased;
        self.velocity =
            (self.target - self.from) * 5.0 * remaining.powi(4) / self.duration.as_secs_f32();
        self.value
    }

    /// Applies an async completion only when it still belongs to this transition.
    // Reason: later async exits consume this guard; remove after TASK-006 and TASK-009 land.
    #[allow(dead_code)]
    pub(crate) fn finish_if_current(&mut self, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        self.finish();
        true
    }

    fn finish(&mut self) {
        self.value = self.target;
        self.velocity = 0.0;
        self.phase = if self.target > 0.0 {
            MotionPhase::Visible
        } else {
            MotionPhase::Hidden
        };
    }
}
