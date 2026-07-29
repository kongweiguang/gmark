<!-- @author kongweiguang -->

# Architecture refactor baseline

This record fixes the compatibility baseline for the multi-worktree architecture
refactor. It is intentionally kept separate from the target architecture document.

## Source baseline

- Original branch: `main`
- Original commit: `b8ddd21239d3d00922684297d5d61158a3dae7b0`
- Integration branch: `kong/gmark-arch-integration`
- The original worktree remains unchanged until final acceptance.

## Baseline verification

- `cargo test --workspace --all-features --locked -j 1`: passed.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`: passed.
- `cargo fmt --all -- --check`: passed.
- `cargo run -p xtask -- quality`: failed on pre-existing file-length,
  production-inline-test, and unexplained-lint-allow findings. These findings are
  migration work, not regressions introduced by the scaffold.
- `cargo check -p gmark --locked --no-default-features`: failed in
  `src/components/markdown/code_highlight.rs`. Repairing this feature combination is
  the only intentionally additive behavior permitted by the plan.
- After adding the six crate scaffolds, `cargo check --workspace --all-features`
  passed.

## Compatibility evidence to preserve

- Configuration and preferences: `tests/unit/config/tests.rs` and
  `tests/unit/config/preferences.rs`.
- Workspace session schemas v1-v8 and normalization:
  `tests/unit/config/workspace_session.rs`.
- Internationalization catalogs and fallback: `tests/unit/i18n/tests.rs`.
- Markdown model, parsing, serialization, resources, HTML, table and TOC:
  `tests/unit/components/markdown/**` and `tests/markdown_spec_corpus.rs`.
- Source folding, formatting and highlighting:
  `tests/unit/components/markdown/code_highlight.rs`, editor folding tests, and
  document-host tests.
- Resident recovery bytes, replay, source format and compaction:
  `tests/unit/recovery.rs`, `crates/gmark-recovery-codec/tests/**`, and document
  backend contract tests.
- Update manifest, signature, rollout, download and helper handshake:
  `tests/unit/net/update.rs`, `tests/unit/net/update_v2.rs`,
  `tests/unit/editor/update.rs`, and update-helper tests.
- Export HTML/PDF/PNG behavior: `tests/unit/export/**` and
  `tests/unit/editor/export.rs`.

The migration may relocate these tests, but it must retain their public assertions
and fixture bytes.
