<!-- @author kongweiguang -->

# Interactive rendering

This page exercises the renderer-facing capabilities that keep Markdown
portable: semantic search text, collapsible sections, wide tables, rich
clipboard output, and formula structure templates.

## Search and folding

Rendered search should find this sentence without matching the `**` markers,
link destination, or other Markdown punctuation. A heading can be collapsed
without changing this source file.

> [!NOTE]
> Callout content remains semantic text for search and keeps its source range
> when the visual body is collapsed.

## Wide table

| Column | Long URL | CJK 内容 | Code |
| :--- | :--- | :---: | ---: |
| Alpha | https://example.test/a/very/long/path/that/scrolls | 你好世界 | `width: 4096px` |
| Beta | https://example.test/b/another/long/path | 渲染缓存 | `LRU 256 MiB` |

## Formula structure templates

Inline `$x^2 + y^2 = z^2$` and a block formula remain lossless when a
structure action inserts a fraction, root, superscript, or matrix template.

$$
\frac{1}{\sqrt{2}}
$$

The source delimiters and surrounding whitespace are never rewritten by a
render-only action.
