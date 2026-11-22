# E2E media fixtures

Generated, not collected: every file here is written by

```bash
cargo run -p squirrel-media --example make_fixtures --features test-util -- web/e2e/fixtures
```

from `squirrel_media::testutil` (JPEGs with chosen EXIF, a PNG, a one-page PDF and a text file).
They are project content: no photo, document or other material from third parties, except
`faces/`, which mirrors the public-domain NASA portraits of `crates/squirrel-ml/tests/fixtures/faces`
(see its `SOURCES.md`). Regenerate
them with the command above when the generator changes.

| File | What the E2E scenarios rely on |
|---|---|
| `paris-2024.jpg` | 1200 × 800, taken 2024-07-14 18:30:05 +02:00, GPS in Paris, Google Pixel 8, 1/250 s, ƒ/1.7, ISO 64 |
| `lyon-2023.jpg` | 800 × 1200, taken 2023-05-01 09:00:00 (no offset), GPS in Lyon, Apple iPhone 15 |
| `Screenshot_2024-07-15-10-00-00.png` | 1080 × 2400, no EXIF: dated and marked a screenshot from its name |
| `notes.txt` | "Quarterly squirrel budget: acorns, hazelnuts and winter storage." |
| `invoice.pdf` | one page reading "Invoice 2026 for acorn storage" |
| `red-circle.jpg`, `blue-square.jpg`, `green-triangle.jpg` | 640 × 480 plain shapes on white, no EXIF: photo search ("red circle") and tags |
| `meadow.jpg`, `meadow-copy.jpg` | one generated scene, 1600 × 1200 at quality 92 and 1024 × 768 at quality 70, no EXIF: near-identical, grouped as duplicates |
| `faces/<name>-mirrored.jpg` | the four portraits mirrored: with the originals (read from `crates/squirrel-ml/tests/fixtures/faces`), four photos of each person, enough for clustering |
