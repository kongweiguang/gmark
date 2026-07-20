<!-- @author kongweiguang -->

# Markdown specification corpus

`markdown-spec-0.13.4.json` is mechanically generated from the test sources shipped in
`pulldown-cmark 0.13.4`:

- `tests/suite/spec.rs`: 652 CommonMark cases;
- `tests/suite/gfm_table.rs`: 9 GFM table cases;
- `tests/suite/gfm_strikethrough.rs`: 3 GFM strikethrough cases;
- `tests/suite/gfm_tasklist.rs`: 2 GFM task-list cases.

The crate checksum is `e9f068eba8e7071c5f9511831b44f32c740d5adf574e990f946ddb53db2f314e`
and its recorded VCS revision is `38e4d08f14ec4bd9783270e9623db7681ebed968`. The source is MIT licensed;
see the repository `NOTICE`.

Regenerate from an unpacked, checksum-verified crate source:

```powershell
python scripts/import-markdown-spec-corpus.py `
  --suite-dir <pulldown-cmark-0.13.4>/tests/suite `
  --output tests/corpus/markdown-spec-0.13.4.json
```

The importer fails unless all pinned suite counts match. Corpus updates must be reviewed together with
the dependency lock, semantic diffs, source-preservation results, and upstream provenance.
