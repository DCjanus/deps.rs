use badge::{Badge, BadgeOptions, BadgeStyle};
use pretty_assertions::assert_eq;
use serde::Deserialize;
use xml_c14n::{CanonicalizationMode, CanonicalizationOptions, canonicalize_xml};

/// These tests intentionally compare our generated SVG against upstream badge-maker fixtures.
///
/// Fixture source is documented in `tests/fixtures/README.md`.
///
/// Note: some cases are expected to fail until style support and rendering parity are implemented.
#[derive(Debug, Deserialize)]
struct StyleQuery {
    style: BadgeStyle,
}

fn canonicalize(svg: &str, context: &str) -> String {
    canonicalize_xml(
        svg,
        CanonicalizationOptions {
            mode: CanonicalizationMode::Canonical1_1,
            keep_comments: true,
            inclusive_ns_prefixes: vec![],
        },
    )
    .unwrap_or_else(|err| panic!("failed to canonicalize {context}: {err}"))
}

fn assert_fixture_parity(style_query: &str, fixture_file: &str) {
    let style = serde_urlencoded::from_str::<StyleQuery>(style_query)
        .map(|query| query.style)
        .unwrap_or_else(|err| {
            panic!(
                "style parsing is not supported yet for `{style_query}`: {err}. \
This test exists as a TODO parity target against badge-maker fixtures."
            )
        });

    let badge = Badge::new(BadgeOptions {
        subject: "cactus".to_owned(),
        status: "grown".to_owned(),
        color: "#b3e".to_owned(),
        style,
    });

    let expected_raw = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{fixture_file}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|err| panic!("failed to read fixture `{fixture_file}`: {err}"));

    let actual = canonicalize(&badge.to_svg(), "generated SVG");
    let expected = canonicalize(&expected_raw, "fixture SVG");

    assert_eq!(actual, expected);
}

#[test]
fn fixture_flat_message_label_no_logo() {
    assert_fixture_parity("style=flat", "flat.cactus-grown.svg");
}

#[test]
fn fixture_flat_square_message_label_no_logo() {
    assert_fixture_parity("style=flat-square", "flat-square.cactus-grown.svg");
}

#[test]
fn fixture_plastic_message_label_no_logo() {
    assert_fixture_parity("style=plastic", "plastic.cactus-grown.svg");
}

#[test]
fn fixture_for_the_badge_message_label_no_logo() {
    assert_fixture_parity("style=for-the-badge", "for-the-badge.cactus-grown.svg");
}

#[test]
fn fixture_social_message_label_no_logo() {
    assert_fixture_parity("style=social", "social.cactus-grown.svg");
}
