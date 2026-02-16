# Fixture source

These SVG fixtures come from the upstream `badge-maker` snapshot tests in `badges/shields`.

- Source snapshot file: [__snapshots__/make-badge.spec.js](https://github.com/badges/shields/blob/master/__snapshots__/make-badge.spec.js)
- Source test definitions: [badge-maker/lib/make-badge.spec.js](https://github.com/badges/shields/blob/master/badge-maker/lib/make-badge.spec.js)

Extraction notes:

- Extracted from the snapshot entries named:
  - `"flat" template ... message/label, no logo`
  - `"flat-square" template ... message/label, no logo`
  - `"plastic" template ... message/label, no logo`
  - `"for-the-badge" template ... message/label, no logo`
  - `"social" template ... message/label, no logo`
- Input case represented by those entries is `label=cactus`, `message=grown`, `color=#b3e`, `labelColor=#0f0`.
- This fixture set is intentionally used as a parity target for deps.rs `libs/badge`.
- Some tests are expected to fail until unsupported styles and rendering differences are implemented.
