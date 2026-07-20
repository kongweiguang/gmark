<!-- @author kongweiguang -->

# About Icons

These SVG icons are sourced from Iconify and stored locally so the app can embed
them through the GPUI asset source at build time.

| Local file | Iconify icon | Icon set | License |
| --- | --- | --- | --- |
| `workspace/folder.svg` | Original gmark-style rounded folder glyph | gmark | GPL-3.0-or-later |
| `workspace/markdown.svg` | Original gmark-style rounded M↓ glyph | gmark | GPL-3.0-or-later |
| `titlebar/chrome-close.svg` | [`codicon:chrome-close`](https://icon-sets.iconify.design/codicon/chrome-close/) | Codicons by Microsoft Corporation | CC BY 4.0 |
| `titlebar/chrome-minimize.svg` | [`codicon:chrome-minimize`](https://icon-sets.iconify.design/codicon/chrome-minimize/) | Codicons by Microsoft Corporation | CC BY 4.0 |
| `titlebar/chrome-maximize.svg` | [`codicon:chrome-maximize`](https://icon-sets.iconify.design/codicon/chrome-maximize/) | Codicons by Microsoft Corporation | CC BY 4.0 |
| `titlebar/chrome-restore.svg` | [`codicon:chrome-restore`](https://icon-sets.iconify.design/codicon/chrome-restore/) | Codicons by Microsoft Corporation | CC BY 4.0 |
| `editor/tab-pin.svg` | [`lucide:pin`](https://lucide.dev/icons/pin) | Lucide | ISC |
| `ui/*.svg` | Corresponding Files, List, ListOrdered, ListChecks, Heading1/2/3, Quote, Sigma, Search, PanelLeft, PanelBottom, PenLine, Code2, Columns2, Eye, X, Chevron, CornerUpLeft, Ellipsis, CaseSensitive, WholeWord, Regex, Copy, Check, Link, Palette, Image, Keyboard, Type, Plus, Minus, Sun, Moon, Monitor, Save, Sliders, Undo, Redo, Scissors, Clipboard, Power, FileOutput, Refresh, Shield, ShieldAlert, Info, Lightbulb, TriangleAlert, Align, Arrow, Trash and Table glyphs | Lucide-derived | ISC |

The exported SVGs keep `currentColor` fill or stroke so the app can color icons with the
active gmark theme.

`gmark-icon.svg` is the canonical original application icon. Run
`cargo run --offline --example render_app_icons` and `python scripts/package-app-icons.py`
to regenerate all PNG, Windows ICO, macOS ICNS, and Linux application resources.
