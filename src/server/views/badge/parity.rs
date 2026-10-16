use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use base64::{Engine as _, prelude::BASE64_STANDARD};
use resvg::{
    tiny_skia::{Pixmap, Transform},
    usvg::{Options, Tree},
};
use serde::Serialize;

use super::*;
use crate::models::crates::{AnalyzedDependencies, AnalyzedDependency};

#[derive(Serialize)]
struct Case {
    #[serde(skip)]
    id: String,
    label: String,
    message: String,
    color: String,
    style: String,
    #[serde(skip)]
    actual: String,
}

fn outcome(
    total: usize,
    outdated: usize,
    vulnerability: Option<bool>,
) -> AnalyzeDependenciesOutcome {
    let mut deps = AnalyzedDependencies {
        main: Default::default(),
        dev: Default::default(),
        build: Default::default(),
    };
    for index in 0..total {
        let mut dep = AnalyzedDependency::new("*".parse().unwrap());
        let latest = if vulnerability == Some(true) {
            "1.0.0"
        } else {
            "2.0.0"
        };
        dep.latest = Some(latest.parse().unwrap());
        dep.latest_that_matches = Some(
            if index < outdated { "1.0.0" } else { latest }
                .parse()
                .unwrap(),
        );
        if index == 0 && vulnerability.is_some() {
            // Synthetic advisory: no network or changing advisory database involved.
            dep.vulnerabilities.push(
                r#"```toml
[advisory]
id = "RUSTSEC-2026-0001"
package = "fixture"
date = "2026-01-01"
[versions]
patched = [">= 2.0.0"]
```
# Synthetic badge fixture
Only used to exercise dependency-status presentation.
"#
                .parse()
                .unwrap(),
            );
        }
        deps.main
            .insert(format!("fixture-{index}").parse().unwrap(), dep);
    }
    AnalyzeDependenciesOutcome {
        crates: vec![("fixture".parse().unwrap(), deps)],
        duration: Duration::ZERO,
    }
}

fn cases() -> Vec<Case> {
    let states = [
        ("unknown", None, "unknown", "#9f9f9f"),
        ("none", Some(outcome(0, 0, None)), "none", "#4c1"),
        ("current", Some(outcome(3, 0, None)), "up to date", "#4c1"),
        (
            "insecure",
            Some(outcome(1, 0, Some(true))),
            "insecure",
            "#e05d44",
        ),
        (
            "maybe-insecure",
            Some(outcome(1, 0, Some(false))),
            "maybe insecure",
            "#8b1",
        ),
        (
            "outdated-1",
            Some(outcome(1, 1, None)),
            "1 of 1 outdated",
            "#dfb317",
        ),
        (
            "outdated-9",
            Some(outcome(10, 9, None)),
            "9 of 10 outdated",
            "#dfb317",
        ),
        (
            "outdated-99",
            Some(outcome(100, 99, None)),
            "99 of 100 outdated",
            "#dfb317",
        ),
        (
            "outdated-999",
            Some(outcome(1000, 999, None)),
            "999 of 1000 outdated",
            "#dfb317",
        ),
    ];
    let labels = [
        ("default", "", "dependencies"),
        ("compact", "compact=true", "deps"),
        ("custom", "subject=workspace%20deps", "workspace deps"),
        (
            "escaped",
            "subject=deps%20%26%20%3Ctools%3E",
            "deps & <tools>",
        ),
        ("unicode", "subject=caf%C3%A9%20%CE%A9", "café Ω"),
        ("empty-subject", "subject=", "dependencies"),
    ];
    let mut cases = Vec::new();
    for (state, outcome, message, color) in &states {
        for style in ["flat", "flat-square", "plastic", "for-the-badge", "social"] {
            for (label_id, query, label) in labels {
                let knobs = ExtraConfig::from_query_string(Some(&format!("style={style}&{query}")));
                cases.push(Case {
                    id: format!("{state}-{style}-{label_id}"),
                    label: label.to_owned(),
                    message: (*message).to_owned(),
                    color: (*color).to_owned(),
                    style: style.to_owned(),
                    actual: render_svg(outcome.as_ref(), knobs),
                });
            }
        }
    }
    cases
}

fn raster(svg: &str, options: &Options<'_>) -> Pixmap {
    let tree = Tree::from_str(svg, options).expect("valid badge SVG");
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height()).unwrap();
    resvg::render(&tree, Transform::default(), &mut pixmap.as_mut());
    pixmap
}

fn image_data(pixmap: &Pixmap) -> String {
    format!(
        "data:image/png;base64,{}",
        BASE64_STANDARD.encode(pixmap.encode_png().unwrap())
    )
}

#[test]
fn badges_match_shields_reference() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cases = cases();
    let mut child = Command::new("bun")
        .current_dir(root.join("tools/badge-parity"))
        .args(["--no-install", "reference.js"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("install Bun and run bun install --cwd tools/badge-parity --frozen-lockfile");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Shields reference failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let references: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), references.len());

    let mut options = Options::default();
    let fonts = options.fontdb_mut();
    fonts.load_font_data(
        include_bytes!("../../../../tools/badge-parity/fonts/DejaVuSans.ttf").to_vec(),
    );
    fonts.load_font_data(
        include_bytes!("../../../../tools/badge-parity/fonts/DejaVuSans-Bold.ttf").to_vec(),
    );
    fonts.set_sans_serif_family("DejaVu Sans");
    fonts.set_serif_family("DejaVu Sans");
    // Fail if a broken font setup silently removes all text from comparisons.
    let probe = raster(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="20"><text y="15" font-family="sans-serif">text</text></svg>"#,
        &options,
    );
    assert!(probe.data().as_chunks::<4>().0.iter().any(|p| p[3] != 0));

    let report = root.join("target/badge-parity");
    fs::create_dir_all(&report).unwrap();
    let mut rows = String::new();
    let mut failures = Vec::new();
    let mut results = Vec::new();
    for (case, reference) in cases.iter().zip(&references) {
        let actual = raster(&case.actual, &options);
        let expected = raster(reference, &options);
        let dimensions_match =
            actual.width() == expected.width() && actual.height() == expected.height();
        let matches = dimensions_match && actual.data() == expected.data();
        if !matches {
            failures.push(case.id.as_str());
        }
        let mut diff = Pixmap::new(
            actual.width().max(expected.width()),
            actual.height().max(expected.height()),
        )
        .unwrap();
        let mut different_pixels = 0;
        for y in 0..diff.height() {
            for x in 0..diff.width() {
                let actual_pixel =
                    (x < actual.width() && y < actual.height()).then(|| actual.pixel(x, y));
                let expected_pixel =
                    (x < expected.width() && y < expected.height()).then(|| expected.pixel(x, y));
                if actual_pixel != expected_pixel {
                    different_pixels += 1;
                    let index = (y * diff.width() + x) as usize;
                    diff.pixels_mut()[index] =
                        resvg::tiny_skia::PremultipliedColorU8::from_rgba(255, 0, 255, 255)
                            .unwrap();
                }
            }
        }
        results.push(serde_json::json!({
            "id": case.id, "input": case, "matches": matches,
            "actual_dimensions": [actual.width(), actual.height()],
            "reference_dimensions": [expected.width(), expected.height()],
            "different_pixels": different_pixels,
        }));
        fs::write(report.join(format!("{}-actual.svg", case.id)), &case.actual).unwrap();
        fs::write(report.join(format!("{}-reference.svg", case.id)), reference).unwrap();
        rows.push_str(&format!(
            "<tr><td>{}<br><a href='{}-actual.svg'>Actual SVG</a> · <a href='{}-reference.svg'>Reference SVG</a></td><td>{}<br>{} differing pixels</td><td><img alt='deps.rs' src='{}'><br>{}×{}</td><td><img alt='Shields' src='{}'><br>{}×{}</td><td><img alt='Difference, magenta marks unequal pixels' src='{}'></td></tr>",
            case.id, case.id,
            case.id, if matches {"PASS"} else {"FAIL"}, different_pixels, image_data(&actual), actual.width(), actual.height(),
            image_data(&expected), expected.width(), expected.height(), image_data(&diff),
        ));
    }
    let sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root)
        .output()
        .unwrap();
    fs::write(
        report.join("results.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
    fs::copy(root.join("Cargo.lock"), report.join("Cargo.lock")).unwrap();
    fs::copy(
        root.join("tools/badge-parity/bun.lock"),
        report.join("bun.lock"),
    )
    .unwrap();
    let html = format!(
        "<!doctype html><html lang='en'><meta charset='utf-8'><meta name='viewport' content='width=device-width, initial-scale=1'><title>deps.rs badge parity</title><style>body{{font:16px sans-serif;margin:2rem}}table{{border-collapse:collapse}}td,th{{padding:.6rem;border:1px solid #ccc}}img{{image-rendering:pixelated;max-width:none}}img:hover{{transform:scale(3);transform-origin:left center;position:relative;z-index:1}}</style><h1>deps.rs badge parity</h1><p>{} cases; {} failures. Commit: {}.</p><p>Reference: badge-maker 6.0.0. Rasterizer: resvg 0.48.1. Fonts: bundled DejaVu Sans 2.37 regular/bold. Zero RGBA tolerance; width and height must match. Hover over images to magnify. Magenta marks unequal pixels. <a href='results.json'>Inputs and results</a> · <a href='Cargo.lock'>Rust dependency lock</a> · <a href='bun.lock'>Reference dependency lock</a>.</p><p>This report compares the actual deps.rs render_svg path using synthetic dependency analyses. Browser-specific font rendering and live Shields deployment freshness are outside this check.</p><table><thead><tr><th>Case</th><th>Result</th><th>deps.rs</th><th>Shields reference</th><th>Difference</th></tr></thead><tbody>{rows}</tbody></table></html>",
        cases.len(),
        failures.len(),
        String::from_utf8_lossy(&sha.stdout).trim(),
    );
    fs::write(report.join("index.html"), html).unwrap();
    assert!(
        failures.is_empty(),
        "badge parity failures: {failures:?}; report: {}",
        report.join("index.html").display()
    );
}
