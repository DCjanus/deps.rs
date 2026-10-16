# Rasterization test fonts

`DejaVuSans.ttf` and `DejaVuSans-Bold.ttf` are unmodified files from
[DejaVu Fonts 2.37](https://github.com/dejavu-fonts/dejavu-fonts/releases/tag/version_2_37),
specifically the `dejavu-fonts-ttf-2.37.tar.bz2` release archive. The accompanying
license is preserved in [LICENSE](LICENSE).

Tests load only these bundled fonts, never host fonts. Both renderers use the
same DejaVu Sans regular/bold fallback, including for Helvetica-based social
badges. This makes text comparisons reproducible across machines; it does not
claim that every browser or installed font produces the same pixels.

These assets are used only by the badge parity test. Production badge width
calculation remains inside badge-maker-rs and its upstream anafanafo tables.
