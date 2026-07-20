//! @author kongweiguang
//! Bench: render loop
//!
//! Higher-level benchmark that simulates one frame of gmark's editor
//! re-rendering a document of N visible blocks. Every commit on
//! `perf/editor-render` (theme/i18n Arc clone, SharedString display text,
//! GraphemeCursor, blink throttle, projection cache, monotonic build text
//! runs) participates in the per-frame hot path. This bench combines all
//! of them so the speedup represents the realistic frame-time win on a
//! large document, not the isolated per-operation win.
//!
//! What one simulated frame does per block:
//!  1. Clone the global Theme   (Arc bump vs ~2 KB deep clone).
//!  2. Clone the global I18n    (Arc bump vs 137 String allocs).
//!  3. Clone the display text   (Arc bump vs full String alloc + into).
//!  4. Check the projection key (3-tuple PartialEq vs full rebuild).
//!  5. Decide blink notify      (elapsed gate vs unconditional).
//!  6. Build text runs          (monotonic span_idx vs 4× per-boundary find).
//!
//! Two block counts (50 and 200) bracket the typical / heavy document
//! scenarios the user observed in debug mode. The "current" path is
//! whatever the perf/editor-render branch leaves in place; the "baseline"
//! is the pre-commit version of each step.

use std::hint::black_box;
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use gpui::SharedString;

mod common;
use common::{MockFragment, MockI18nStrings, MockSpan, MockTheme, mock_fragments, mock_spans};

/// What one block's worth of per-frame work costs in the *pre-commit* code.
fn frame_step_baseline(
    theme: &MockTheme,
    strings: &MockI18nStrings,
    display_text: &str,
    spans: &[MockSpan],
    fragments: &[MockFragment],
    text_len: usize,
) -> usize {
    // 1. Theme deep clone.
    let _t = black_box(theme.clone());
    // 2. I18nStrings deep clone (137 String allocs).
    let _s = black_box(strings.clone());
    // 3. Display text: fresh String + conversion to SharedString.
    let owned: String = display_text.to_string();
    let _shared: SharedString = owned.into();
    // 4. Projection: unconditional full rebuild.
    let _proj = simulate_projection_build(fragments);
    // 5. Blink tick: unconditional "do work".
    let _notify = true;
    // 6. build_text_runs: four linear span scans per boundary.
    old_build_text_runs(spans, text_len)
}

/// What one block's worth of per-frame work costs in the *post-commit* code.
fn frame_step_current(
    theme_arc: &Arc<MockTheme>,
    strings_arc: &Arc<MockI18nStrings>,
    cached_text: &SharedString,
    cached_key: &(bool, Range<usize>, Option<Range<usize>>),
    current_key: &(bool, Range<usize>, Option<Range<usize>>),
    epoch: Instant,
    spans: &[MockSpan],
    text_len: usize,
) -> usize {
    // 1. Theme Arc clone.
    let _t = black_box(theme_arc.clone());
    // 2. I18nStrings Arc clone.
    let _s = black_box(strings_arc.clone());
    // 3. Display text Arc bump.
    let _shared = black_box(cached_text.clone());
    // 4. Projection cache hit ⇒ no rebuild.
    let _hit = black_box(cached_key) == black_box(current_key);
    // 5. Blink throttle gate.
    let _notify = epoch.elapsed().as_secs_f32() >= 0.5;
    // 6. build_text_runs: monotonic span_idx.
    new_build_text_runs(spans, text_len)
}

// --- inlined algorithms (copies from the per-commit benches so this file
// can run standalone without a workspace-shared helper) ---

fn old_build_text_runs(spans: &[MockSpan], text_len: usize) -> usize {
    let mut boundaries = vec![0, text_len];
    for s in spans {
        boundaries.push(s.range.start);
        boundaries.push(s.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut acc = 0usize;
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        let active = |o: usize| spans.iter().find(|s| s.range.start <= o && o < s.range.end);
        let style = active(start).map(|s| s.style).unwrap_or_default();
        let html = active(start).and_then(|s| s.html_style);
        let is_link = active(start).and_then(|s| s.link).is_some();
        let is_foot = active(start).and_then(|s| s.footnote).is_some();
        acc += style.bold as usize
            + style.italic as usize
            + html.unwrap_or(0) as usize
            + is_link as usize
            + is_foot as usize;
    }
    acc
}

fn new_build_text_runs(spans: &[MockSpan], text_len: usize) -> usize {
    let mut boundaries = vec![0, text_len];
    for s in spans {
        boundaries.push(s.range.start);
        boundaries.push(s.range.end);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut acc = 0usize;
    let mut span_idx = 0usize;
    for pair in boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }
        while span_idx < spans.len() && spans[span_idx].range.end <= start {
            span_idx += 1;
        }
        let active = spans
            .get(span_idx)
            .filter(|s| s.range.start <= start && start < s.range.end);
        let style = active.map(|s| s.style).unwrap_or_default();
        let html = active.and_then(|s| s.html_style);
        let is_link = active.and_then(|s| s.link).is_some();
        let is_foot = active.and_then(|s| s.footnote).is_some();
        acc += style.bold as usize
            + style.italic as usize
            + html.unwrap_or(0) as usize
            + is_link as usize
            + is_foot as usize;
    }
    acc
}

fn simulate_projection_build(fragments: &[MockFragment]) -> Option<(usize, Vec<usize>)> {
    let clean_len: usize = fragments.iter().map(|f| f.text.len()).sum();
    let mut display_to_clean: Vec<usize> = Vec::with_capacity(clean_len + 1);
    let mut clean_to_display: Vec<usize> = vec![0; clean_len + 1];
    let mut display_cursor = 0usize;
    let mut clean_cursor = 0usize;
    let mut any_expanded = false;
    for f in fragments {
        let len = f.text.len();
        if f.has_link {
            for _ in 0..2 {
                display_to_clean.push(clean_cursor);
            }
            display_cursor += 2;
            any_expanded = true;
        }
        for offset in 0..=len {
            clean_to_display[clean_cursor + offset] = display_cursor + offset;
        }
        for offset in 1..=len {
            display_to_clean.push(clean_cursor + offset);
        }
        display_cursor += len;
        clean_cursor += len;
    }
    any_expanded.then_some((display_cursor, clean_to_display))
}

/// Pick the run of rows whose extent intersects the viewport band, from each
/// row's content-space top. Mirrors the shape of `Editor::rendered_window`.
fn windowed_run(tops: &[f32], row_height: f32, band_top: f32, band_bottom: f32) -> (usize, usize) {
    let n = tops.len();
    let start = tops
        .iter()
        .position(|&top| top + row_height >= band_top)
        .unwrap_or(n);
    let end = tops
        .iter()
        .rposition(|&top| top <= band_bottom)
        .map(|i| i + 1)
        .unwrap_or(0);
    (start, end.max(start))
}

/// Stand-in for the per-row element construction skipped when a row is culled:
/// a small allocation plus arithmetic, black-boxed so it is not optimized away.
fn simulated_row_build(seed: usize) -> usize {
    let mut buf = Vec::with_capacity(8);
    for k in 0..8 {
        buf.push(black_box(seed.wrapping_mul(31).wrapping_add(k)));
    }
    black_box(buf.iter().sum())
}

fn render_loop(c: &mut Criterion) {
    // Per-block fixtures.
    let theme_owned = MockTheme::new();
    let theme_arc = Arc::new(MockTheme::new());
    let strings_owned = MockI18nStrings::new();
    let strings_arc = Arc::new(MockI18nStrings::new());
    let text = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(8);
    let cached_text = SharedString::from(text.clone());
    let spans = mock_spans(30);
    let span_text_len = spans.last().unwrap().range.end;
    let fragments = mock_fragments(20);
    let cached_key: (bool, Range<usize>, Option<Range<usize>>) = (true, 5..5, None);
    let current_key = cached_key.clone();
    let epoch = Instant::now();

    let mut group = c.benchmark_group("render loop (per frame)");
    for &n_blocks in &[50usize, 200usize, 1_000usize, 5_000usize] {
        group.bench_with_input(
            BenchmarkId::new("baseline", n_blocks),
            &n_blocks,
            |b, &n_blocks| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for _ in 0..n_blocks {
                        acc += frame_step_baseline(
                            &theme_owned,
                            &strings_owned,
                            &text,
                            &spans,
                            &fragments,
                            span_text_len,
                        );
                    }
                    black_box(acc);
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("current", n_blocks),
            &n_blocks,
            |b, &n_blocks| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for _ in 0..n_blocks {
                        acc += frame_step_current(
                            &theme_arc,
                            &strings_arc,
                            &cached_text,
                            &cached_key,
                            &current_key,
                            epoch,
                            &spans,
                            span_text_len,
                        );
                    }
                    black_box(acc);
                });
            },
        );
    }
    group.finish();

    // Viewport culling: per-row build work for every row vs only the on-screen
    // window. The saving grows with document length.
    let mut culling = c.benchmark_group("viewport culling (per frame)");
    for &n_blocks in &[1_000usize, 5_000usize, 15_000usize, 30_000usize] {
        let row_height = 50.0f32;
        let tops: Vec<f32> = (0..n_blocks).map(|i| i as f32 * row_height).collect();
        // A ~800px viewport band parked in the middle of the document.
        let band_top = (n_blocks as f32 * row_height) * 0.5;
        let band_bottom = band_top + 800.0;

        culling.bench_with_input(
            BenchmarkId::new("baseline (all rows)", n_blocks),
            &n_blocks,
            |b, &n_blocks| {
                b.iter(|| {
                    let mut acc = 0usize;
                    for i in 0..n_blocks {
                        acc += simulated_row_build(i);
                    }
                    black_box(acc);
                });
            },
        );
        culling.bench_with_input(
            BenchmarkId::new("current (windowed)", n_blocks),
            &n_blocks,
            |b, &_n_blocks| {
                b.iter(|| {
                    let (start, end) =
                        windowed_run(black_box(&tops), row_height, band_top, band_bottom);
                    let mut acc = 0usize;
                    for i in start..end {
                        acc += simulated_row_build(i);
                    }
                    black_box(acc);
                });
            },
        );

        // Windowed path with ~1-in-20 unmeasured rows (unfocused math/mermaid),
        // to check culling stays active when estimates are mixed in.
        let measured: Vec<bool> = (0..n_blocks).map(|i| i % 20 != 0).collect();
        culling.bench_with_input(
            BenchmarkId::new("current (windowed, mixed structures)", n_blocks),
            &n_blocks,
            |b, &n_blocks| {
                b.iter(|| {
                    let mut tops_filled = vec![0.0f32; n_blocks];
                    let mut cursor = 0.0f32;
                    for i in 0..n_blocks {
                        if measured[i] {
                            tops_filled[i] = i as f32 * row_height;
                            cursor = tops_filled[i] + row_height;
                        } else {
                            tops_filled[i] = cursor;
                            cursor += row_height;
                        }
                    }
                    let (start, end) =
                        windowed_run(black_box(&tops_filled), row_height, band_top, band_bottom);
                    let mut acc = 0usize;
                    for i in start..end {
                        acc += simulated_row_build(i);
                    }
                    black_box(acc);
                });
            },
        );
    }
    culling.finish();
}

criterion_group!(benches, render_loop);
criterion_main!(benches);
