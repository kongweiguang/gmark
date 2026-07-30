<!-- @author kongweiguang -->

# GMark architecture

This document is the architecture contract for the current workspace. It
records the ownership boundaries implemented by the refactor and the rules
enforced by `xtask`; changes that cross a boundary must update code, tests, and
this contract together.

## Target directory layout

The six domain packages are workspace members under `crates/` and are direct
dependencies of the root `gmark` package.

| Domain | Target directory | Current responsibility boundary |
| --- | --- | --- |
| Configuration | `crates/gmark-config/src/` | Configuration directories, preferences, recent files, and workspace-session codecs. |
| Internationalization | `crates/gmark-i18n/src/` | Locale metadata, catalogs, JSONC packs, built-ins, and translation fallback. |
| Markdown | `crates/gmark-markdown/src/` | Markdown blocks, inline content, parsing, serialization, resources, HTML, tables, and TOC. |
| Source tools | `crates/gmark-source-tools/src/` | Folding, formatting, highlighting, language selection, incremental work, and ranges. |
| Export | `crates/gmark-export/src/` | Export HTML, resources, images, Chromium integration, math, markup, and export theme handling. |
| Update core | `crates/gmark-update-core/src/` | Update envelopes, manifests, policy, protocol, staging, and update errors. |

The root package remains at `src/`. It is the composition boundary for the
application shell, GPUI, accessibility, window/platform integration, editor,
and app-level adapters. Domain crates expose reusable functionality to that
shell; they do not import the shell back.

The main package is organized around these composition roots:

```text
src/
├── app/            bootstrap, menus, preferences, diagnostics, update coordination
├── platform/       windows, single-instance handling, URLs, accessibility
├── ui/             actions, controls, localization Global, theme tokens
├── adapters/       document, resource, recovery, and export IO
├── document_host/  GPUI host for resident, paged, and structured documents
├── editor/         editor state, commands, input, workspace, and rendering sources
├── components/     one lower-level registry shared by Editor and DocumentHost
├── net/            HTTP transport and update-download adapter
├── source_tools/   shell adapter around the pure source-tools domain
├── spellcheck.rs   local-only spelling service
├── lib.rs
├── main.rs
└── bin/
```

`components` is not an orchestration layer. It mounts the shared Block,
Markdown UI-adapter, LaTeX, and Mermaid sources once so `editor` and
`document_host` consume identical GPUI entity types without depending on each
other. Application actions and controls themselves are owned by `ui`.

## Dependency direction

Cargo metadata currently establishes these workspace edges:

```text
gmark
  -> gmark-config, gmark-i18n, gmark-markdown, gmark-source-tools
  -> gmark-export, gmark-update-core
  -> gmark-document-core, gmark-document, gmark-document-runtime
  -> gmark-json-graph, gmark-paged-document, gmark-recovery-codec

gmark-config -> gmark-document-core
gmark-export -> gmark-markdown, gmark-source-tools
gmark-document-runtime
  -> gmark-document-core, gmark-document, gmark-paged-document
  -> gmark-recovery-codec
```

`gmark-i18n`, `gmark-markdown`, `gmark-source-tools`, and
`gmark-update-core` have no current direct workspace-crate dependencies.
External library dependencies are not represented in the diagram.

The enforced direction is:

- `gmark-config`, `gmark-i18n`, `gmark-markdown`, `gmark-source-tools`,
  `gmark-export`, and `gmark-update-core` must not directly depend on the
  root `gmark` package.
- Those six domain crates must not directly depend on GPUI, AccessKit, or
  window-platform packages/APIs. The check includes Cargo dependency names
  and source paths such as `std::os::windows`.
- `gmark-export` may directly depend on workspace crates only through
  `gmark-markdown` and `gmark-source-tools`.
- Root `src/ui`, `src/platform`, `src/adapters`, and `src/document_host`
  boundaries must not reach back into root `editor` or `app` modules.
  Existing root module boundaries for components, config, export, net, and
  theme remain covered as well.

The existing `gmark-document`, `gmark-paged-document`, and
`gmark-recovery-codec` packages remain protected from direct GPUI, AccessKit,
and window-platform dependencies.

## Main package boundary

Only the root application package owns desktop UI and platform integration.
It may depend outward on the domain crates, while domain packages stay usable
without an application-shell dependency. This keeps configuration, locale
data, Markdown handling, source analysis, export work, and update protocol
usable from non-windowed callers.

Within the root package, UI/platform/adapters/document-host code may use
lower-level domain services, but editor/app orchestration must not flow back
into those lower layers. This is a source-module rule, not a text search:
the gate tokenizes Rust source and resolves `crate::`/`super::` root module
paths while ignoring comments and string literals.

## Compatibility invariants

The refactor must preserve these observable contracts recorded in the
architecture baseline:

- Configuration and preferences behavior, including workspace-session schemas
  v1-v8 and their normalization.
- Internationalization catalogs, selectable language metadata, and fallback.
- Markdown model, parsing, serialization, resources, HTML, table, and TOC
  behavior.
- Source folding, formatting, and highlighting behavior.
- Resident recovery bytes, replay, source format, and compaction behavior.
- Update manifest, signature, rollout, download, helper handshake, and
  application/restart behavior.
- HTML, PDF, and PNG export behavior.

Moving code or tests is allowed only when the public assertions and fixture
bytes that evidence these contracts remain intact.

## Quality contract

`cargo run -p xtask -- quality` applies these rules:

| Gate | Enforced rule |
| --- | --- |
| `source-size` | Every manually maintained Rust file, including `tests/`, `benches/`, `examples/`, `fuzz/`, and `xtask/`, warns above 500 lines and fails above 800 lines. Generated sources plus `vendor/` and `target/` are excluded. |
| `test-layout` | Production `src/` paths cannot contain test fixtures, inline `mod tests { ... }`, inline `#[cfg(test)]` modules, or `#[test]` bodies. |
| `architecture` | Cargo-metadata dependency boundaries, tokenized source-module boundaries, no implementation `include!`, no numbered source filenames, no unreachable Rust modules, and adjacent lint-allow justification/removal conditions. |
| `authors` | Manually maintained Rust, documentation, scripts, workflows, and manifests must retain `@author kongweiguang`. |

Lint `allow` attributes require the immediately preceding `//` comment to
state both a reason and a concrete removal condition. The gate intentionally
does not make existing violations pass by widening exceptions.

## Verification matrix

| Check | Evidence it protects |
| --- | --- |
| `cargo test -p xtask --test quality_gates --locked` | Positive and negative fixtures for every quality and architecture rule, including Cargo metadata edges and source-path false-positive cases. |
| `cargo test -p xtask --locked` | The full `xtask` package test surface. |
| `cargo fmt --all -- --check` | Formatting across the workspace. |
| `git diff --check` | Whitespace and patch integrity. |
| `cargo run -p xtask -- quality` | The real-worktree architecture, authorship, test-layout, and hard source-size gates. The command must be green for integration. |
