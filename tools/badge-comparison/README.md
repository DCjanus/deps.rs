# Badge renderer comparison

This temporary tool compares the legacy in-tree renderer with `badge-maker-rs`
using the badge states and styles exposed by deps.rs.

Run it from the repository root:

```console
cargo run -p badge-comparison -- --output badge-comparison-results
```

The output contains source SVGs, 3x PNG renders, red-channel difference images,
machine-readable metrics, and a Markdown report. The images are composited onto
white and placed on an equal-sized, top-left-aligned canvas before comparison.

The metrics are intended to rank changes for human review, not to serve as a
pass/fail threshold. In particular, SSIM is sensitive to intentional shifts in
text positioning and badge width.
