# deps.rs badge parity

Run from the repository root with Rust and Bun 1.3.11 installed:

```sh
bun install --cwd tools/badge-parity --frozen-lockfile
cargo test badges_match_shields_reference
```

Open `target/badge-parity/index.html` for a self-contained HTML comparison.
The report and the original SVG pairs are also uploaded as the
`badge-parity-report` CI artifact, including when pixel comparison fails.
Generated output is not committed. Static hosting can publish this directory
without changing the test command.

The 270 cases exercise the actual deps.rs `render_svg` path with synthetic,
deterministic dependency analyses. They cover all six status branches, outdated
counts across digit boundaries, all five styles, default/compact/custom labels,
escaped and Unicode labels, and empty-subject fallback. Expected labels,
messages, and colors are specified independently of the Rust rendering options.

The reference is the official JavaScript `badge-maker` 6.0.0 package, fixed in
`package.json` and `bun.lock`. It is associated with
[badges/shields@b6dd9db77a37580d6c0f65dc6d88fe2080032cc1](https://github.com/badges/shields/commit/b6dd9db77a37580d6c0f65dc6d88fe2080032cc1).
This reproducible test does not query the changing live Shields service or
assert deployment freshness. Rust dependencies are recorded in `Cargo.lock`.

Both SVGs are rasterized at native size with resvg and bundled DejaVu Sans 2.37
regular/bold fonts. Width, height, and every RGBA pixel must match. The same
fallback fonts are used for both renderers, including social badges, rather
than depending on the machine's fonts. This is not a claim of identical pixels
in every browser/font environment. Font sources and license are in `fonts/`.
