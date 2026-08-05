// @author kongweiguang

use std::time::{Duration, Instant};

use super::motion::{MotionOrigin, MotionPhase, MotionTokens, TransitionState};

#[test]
fn transition_retargets_from_the_live_presentation_value() {
    let start = Instant::now();
    let mut transition = TransitionState::hidden();
    let first_generation = transition.retarget(
        1.0,
        start,
        Duration::from_millis(200),
        MotionOrigin::Pointer,
    );
    let midpoint = start + Duration::from_millis(100);
    let live_value = transition.sample(midpoint);

    let second_generation = transition.retarget(
        0.0,
        midpoint,
        Duration::from_millis(160),
        MotionOrigin::Pointer,
    );
    assert!(second_generation > first_generation);
    assert!((transition.value() - live_value).abs() < f32::EPSILON);
    assert_eq!(transition.phase(), MotionPhase::Exiting);
    assert!(!transition.finish_if_current(first_generation));

    let next_frame = transition.sample(midpoint + Duration::from_millis(16));
    assert!(next_frame <= live_value);
    assert!(next_frame >= 0.0);
}

#[test]
fn reduced_motion_tokens_finish_without_scheduling_animation() {
    let tokens = MotionTokens::default().resolved(true);
    assert_eq!(tokens.switch, Duration::ZERO);
    assert_eq!(tokens.drawer_enter, Duration::ZERO);
    assert_eq!(tokens.discrete_wheel, Duration::ZERO);

    let now = Instant::now();
    let mut transition = TransitionState::hidden();
    transition.retarget(1.0, now, tokens.switch, MotionOrigin::Keyboard);
    assert_eq!(transition.phase(), MotionPhase::Visible);
    assert_eq!(transition.value(), 1.0);
    assert!(!transition.is_active());
}
