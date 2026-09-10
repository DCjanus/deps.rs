# Badge renderer comparison

Rendered at 3x with resvg 0.48.1 and composited onto white. SSIM is luminance-based; changed pixels use a per-channel tolerance of 2/255; MAE is normalized RGB absolute error. Results are sorted by changed-pixel ratio.

The legacy renderer cannot produce valid XML when the subject contains `&`; the new renderer escapes it, so that semantic improvement is covered separately rather than assigned a visual similarity score.

| Case | Old | New | SSIM | Changed pixels | RGB MAE |
| --- | ---: | ---: | ---: | ---: | ---: |
| flat--long-subject | 274x20 | 264x20 | 0.3451 | 55.90% | 0.1265 |
| flat--outdated | 192x20 | 188x20 | 0.4261 | 53.51% | 0.0934 |
| flat--maybe-insecure | 186x20 | 182x20 | 0.4537 | 51.56% | 0.0950 |
| flat--up-to-date | 154x20 | 152x20 | 0.4682 | 49.95% | 0.0941 |
| flat--insecure | 142x20 | 142x20 | 0.4806 | 49.00% | 0.0792 |
| flat--unknown | 144x20 | 146x20 | 0.5294 | 48.63% | 0.0740 |
| flat--none | 121x20 | 122x20 | 0.4830 | 47.92% | 0.0844 |
| flat-square--outdated | 192x20 | 188x20 | 0.6149 | 19.85% | 0.0671 |
| flat-square--maybe-insecure | 186x20 | 182x20 | 0.6249 | 19.24% | 0.0728 |
| flat-square--up-to-date | 154x20 | 152x20 | 0.6431 | 19.00% | 0.0757 |
| for-the-badge--outdated | 256x28 | 270x28 | 0.7048 | 18.32% | 0.0744 |
| for-the-badge--maybe-insecure | 250x28 | 260x28 | 0.6840 | 17.91% | 0.0832 |
| flat-square--none | 121x20 | 122x20 | 0.6501 | 17.54% | 0.0668 |
| flat-square--insecure | 142x20 | 142x20 | 0.6504 | 17.39% | 0.0578 |
| flat-square--unknown | 144x20 | 146x20 | 0.7324 | 16.68% | 0.0499 |
| for-the-badge--none | 185x28 | 179x28 | 0.7290 | 14.65% | 0.0719 |
| for-the-badge--unknown | 208x28 | 212x28 | 0.7216 | 13.76% | 0.0508 |
| for-the-badge--insecure | 206x28 | 209x28 | 0.7253 | 12.97% | 0.0531 |
| for-the-badge--up-to-date | 218x28 | 222x28 | 0.7325 | 12.80% | 0.0600 |

## Visual comparison

| Case | Legacy | badge-maker-rs 0.2.0 | Difference |
| --- | --- | --- | --- |
| flat--long-subject | ![legacy](./flat--long-subject--old.png) | ![new](./flat--long-subject--new.png) | ![difference](./flat--long-subject--diff.png) |
| flat--outdated | ![legacy](./flat--outdated--old.png) | ![new](./flat--outdated--new.png) | ![difference](./flat--outdated--diff.png) |
| flat--maybe-insecure | ![legacy](./flat--maybe-insecure--old.png) | ![new](./flat--maybe-insecure--new.png) | ![difference](./flat--maybe-insecure--diff.png) |
| flat--up-to-date | ![legacy](./flat--up-to-date--old.png) | ![new](./flat--up-to-date--new.png) | ![difference](./flat--up-to-date--diff.png) |
| flat--insecure | ![legacy](./flat--insecure--old.png) | ![new](./flat--insecure--new.png) | ![difference](./flat--insecure--diff.png) |
| flat--unknown | ![legacy](./flat--unknown--old.png) | ![new](./flat--unknown--new.png) | ![difference](./flat--unknown--diff.png) |
| flat--none | ![legacy](./flat--none--old.png) | ![new](./flat--none--new.png) | ![difference](./flat--none--diff.png) |
| flat-square--outdated | ![legacy](./flat-square--outdated--old.png) | ![new](./flat-square--outdated--new.png) | ![difference](./flat-square--outdated--diff.png) |
| flat-square--maybe-insecure | ![legacy](./flat-square--maybe-insecure--old.png) | ![new](./flat-square--maybe-insecure--new.png) | ![difference](./flat-square--maybe-insecure--diff.png) |
| flat-square--up-to-date | ![legacy](./flat-square--up-to-date--old.png) | ![new](./flat-square--up-to-date--new.png) | ![difference](./flat-square--up-to-date--diff.png) |
| for-the-badge--outdated | ![legacy](./for-the-badge--outdated--old.png) | ![new](./for-the-badge--outdated--new.png) | ![difference](./for-the-badge--outdated--diff.png) |
| for-the-badge--maybe-insecure | ![legacy](./for-the-badge--maybe-insecure--old.png) | ![new](./for-the-badge--maybe-insecure--new.png) | ![difference](./for-the-badge--maybe-insecure--diff.png) |
| flat-square--none | ![legacy](./flat-square--none--old.png) | ![new](./flat-square--none--new.png) | ![difference](./flat-square--none--diff.png) |
| flat-square--insecure | ![legacy](./flat-square--insecure--old.png) | ![new](./flat-square--insecure--new.png) | ![difference](./flat-square--insecure--diff.png) |
| flat-square--unknown | ![legacy](./flat-square--unknown--old.png) | ![new](./flat-square--unknown--new.png) | ![difference](./flat-square--unknown--diff.png) |
| for-the-badge--none | ![legacy](./for-the-badge--none--old.png) | ![new](./for-the-badge--none--new.png) | ![difference](./for-the-badge--none--diff.png) |
| for-the-badge--unknown | ![legacy](./for-the-badge--unknown--old.png) | ![new](./for-the-badge--unknown--new.png) | ![difference](./for-the-badge--unknown--diff.png) |
| for-the-badge--insecure | ![legacy](./for-the-badge--insecure--old.png) | ![new](./for-the-badge--insecure--new.png) | ![difference](./for-the-badge--insecure--diff.png) |
| for-the-badge--up-to-date | ![legacy](./for-the-badge--up-to-date--old.png) | ![new](./for-the-badge--up-to-date--new.png) | ![difference](./for-the-badge--up-to-date--diff.png) |
