// @author kongweiguang

//! Bench: blink throttle
//!
//! Tracks the caret-blink scheduling decision. The original smooth cosine
//! sampled opacity and notified the editor 30×/sec while idle. The current
//! two-phase caret only needs one transition every 500ms, reducing sustained
//! repaint pressure from 30Hz to 2Hz without changing editing semantics.
//!
//! The microbench only measures phase calculation. The actual saving is the
//! 28 avoided editor renders per second and must be verified in a real app.

use std::hint::black_box;
use std::time::Instant;

use criterion::{Criterion, criterion_group, criterion_main};

fn blink_throttle(c: &mut Criterion) {
    let mut group = c.benchmark_group("blink throttle");
    let epoch = Instant::now();
    group.bench_function("baseline (always notify)", |b| {
        b.iter(|| {
            // Pre-commit: unconditional "yes, do the work" decision.
            let should_notify = true;
            black_box(should_notify);
        });
    });
    group.bench_function("current (500ms phase)", |b| {
        b.iter(|| {
            let phase = epoch.elapsed().as_millis() / 500;
            black_box(phase.is_multiple_of(2));
        });
    });
    group.finish();
}

criterion_group!(benches, blink_throttle);
criterion_main!(benches);
