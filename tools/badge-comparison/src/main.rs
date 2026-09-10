use std::{
    cmp::Reverse,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use badge::{Badge, BadgeOptions as OldOptions, BadgeStyle};
use badge_maker_rs::{BadgeOptions as NewOptions, Color, Style, make_badge};
use clap::Parser;
use image::{GrayImage, ImageBuffer, Luma, Rgba, RgbaImage};
use image_compare::{Algorithm, gray_similarity_structure};
use serde::Serialize;

const SCALE: u32 = 3;

#[derive(Parser)]
#[command(about = "Compare deps.rs' legacy badge renderer with badge-maker-rs")]
struct Args {
    #[arg(long, default_value = "badge-comparison-results")]
    output: PathBuf,
}

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    subject: &'static str,
    status: &'static str,
    color: &'static str,
    style: &'static str,
}

#[derive(Serialize)]
struct Metrics {
    case: String,
    old_size: [u32; 2],
    new_size: [u32; 2],
    canvas_size: [u32; 2],
    ssim: f64,
    changed_pixel_ratio: f64,
    mean_absolute_error: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.output.exists() {
        if args.output.read_dir()?.next().is_some() {
            bail!("output directory is not empty: {}", args.output.display());
        }
    } else {
        fs::create_dir_all(&args.output)
            .with_context(|| format!("create {}", args.output.display()))?;
    }

    let mut cases = Vec::new();
    let statuses = [
        ("unknown", "unknown", "#9f9f9f"),
        ("none", "none", "#4c1"),
        ("up-to-date", "up to date", "#4c1"),
        ("outdated", "3 of 10 outdated", "#dfb317"),
        ("maybe-insecure", "maybe insecure", "#8b1"),
        ("insecure", "insecure", "#e05d44"),
    ];
    for (style, _, _) in styles() {
        for (name, status, color) in statuses {
            cases.push(Case {
                name,
                subject: "dependencies",
                status,
                color,
                style,
            });
        }
    }
    cases.push(Case {
        name: "long-subject",
        subject: "workspace dependencies",
        status: "12 of 120 outdated",
        color: "#dfb317",
        style: "flat",
    });

    let mut metrics = Vec::new();
    for case in cases {
        metrics.push(compare(case, &args.output)?);
    }
    metrics.sort_by_key(|m| Reverse((m.changed_pixel_ratio * 1_000_000.0) as u64));

    fs::write(
        args.output.join("metrics.json"),
        serde_json::to_string_pretty(&metrics)?,
    )?;
    fs::write(args.output.join("report.md"), report(&metrics))?;
    println!(
        "wrote {} comparisons to {}",
        metrics.len(),
        args.output.display()
    );
    Ok(())
}

fn styles() -> [(&'static str, BadgeStyle, Style); 3] {
    [
        ("flat", BadgeStyle::Flat, Style::Flat),
        ("flat-square", BadgeStyle::FlatSquare, Style::FlatSquare),
        ("for-the-badge", BadgeStyle::ForTheBadge, Style::ForTheBadge),
    ]
}

fn compare(case: Case, output: &Path) -> Result<Metrics> {
    let (_, old_style, new_style) = styles()
        .into_iter()
        .find(|(name, _, _)| *name == case.style)
        .context("unsupported style")?;
    let old_svg = Badge::new(OldOptions {
        subject: case.subject.into(),
        status: case.status.into(),
        color: case.color.into(),
        style: old_style,
    })
    .to_svg();
    let mut options = NewOptions::new(case.status)
        .label(case.subject)
        .style(new_style)
        .build();
    options.color = Some(Color::literal(case.color));
    let new_svg = make_badge(&options)?;

    let slug = format!("{}--{}", case.style, case.name);
    fs::write(output.join(format!("{slug}--old.svg")), &old_svg)?;
    fs::write(output.join(format!("{slug}--new.svg")), &new_svg)?;

    let old = render(&old_svg)?;
    let new = render(&new_svg)?;
    let old_size = [old.width() / SCALE, old.height() / SCALE];
    let new_size = [new.width() / SCALE, new.height() / SCALE];
    let width = old.width().max(new.width());
    let height = old.height().max(new.height());
    let old = on_white_canvas(&old, width, height);
    let new = on_white_canvas(&new, width, height);
    let diff = diff_image(&old, &new);
    old.save(output.join(format!("{slug}--old.png")))?;
    new.save(output.join(format!("{slug}--new.png")))?;
    diff.save(output.join(format!("{slug}--diff.png")))?;

    let old_gray = grayscale(&old);
    let new_gray = grayscale(&new);
    let ssim = gray_similarity_structure(&Algorithm::MSSIMSimple, &old_gray, &new_gray)?.score;
    let (changed_pixel_ratio, mean_absolute_error) = pixel_metrics(&old, &new);

    Ok(Metrics {
        case: slug,
        old_size,
        new_size,
        canvas_size: [width / SCALE, height / SCALE],
        ssim,
        changed_pixel_ratio,
        mean_absolute_error,
    })
}

fn render(svg: &str) -> Result<RgbaImage> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options)?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width() * SCALE, size.height() * SCALE)
        .context("invalid SVG dimensions")?;
    let transform = resvg::tiny_skia::Transform::from_scale(SCALE as f32, SCALE as f32);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.data().to_vec())
        .context("invalid rendered pixel buffer")
}

fn on_white_canvas(source: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    for (x, y, pixel) in source.enumerate_pixels() {
        let alpha = pixel[3] as u16;
        let blend = |channel: u8| ((channel as u16 * alpha + 255 * (255 - alpha)) / 255) as u8;
        canvas.put_pixel(
            x,
            y,
            Rgba([blend(pixel[0]), blend(pixel[1]), blend(pixel[2]), 255]),
        );
    }
    canvas
}

fn grayscale(image: &RgbaImage) -> GrayImage {
    ImageBuffer::from_fn(image.width(), image.height(), |x, y| {
        let p = image.get_pixel(x, y);
        Luma([((299 * p[0] as u32 + 587 * p[1] as u32 + 114 * p[2] as u32) / 1000) as u8])
    })
}

fn pixel_metrics(old: &RgbaImage, new: &RgbaImage) -> (f64, f64) {
    let mut changed = 0_u64;
    let mut absolute_error = 0_u64;
    for (a, b) in old.pixels().zip(new.pixels()) {
        let differences = [0, 1, 2].map(|channel| a[channel].abs_diff(b[channel]));
        if differences.into_iter().any(|difference| difference > 2) {
            changed += 1;
        }
        absolute_error += differences.into_iter().map(u64::from).sum::<u64>();
    }
    let pixels = u64::from(old.width()) * u64::from(old.height());
    (
        changed as f64 / pixels as f64,
        absolute_error as f64 / (pixels * 3 * 255) as f64,
    )
}

fn diff_image(old: &RgbaImage, new: &RgbaImage) -> RgbaImage {
    ImageBuffer::from_fn(old.width(), old.height(), |x, y| {
        let a = old.get_pixel(x, y);
        let b = new.get_pixel(x, y);
        let delta = [0, 1, 2]
            .into_iter()
            .map(|channel| a[channel].abs_diff(b[channel]))
            .max()
            .unwrap_or_default();
        Rgba([delta, 0, 0, 255])
    })
}

fn report(metrics: &[Metrics]) -> String {
    let mut output = String::from(
        "# Badge renderer comparison\n\nRendered at 3x with resvg 0.48.1 and composited onto white. SSIM is luminance-based; changed pixels use a per-channel tolerance of 2/255; MAE is normalized RGB absolute error. Results are sorted by changed-pixel ratio.\n\nThe legacy renderer cannot produce valid XML when the subject contains `&`; the new renderer escapes it, so that semantic improvement is covered separately rather than assigned a visual similarity score.\n\n| Case | Old | New | SSIM | Changed pixels | RGB MAE |\n| --- | ---: | ---: | ---: | ---: | ---: |\n",
    );
    for metric in metrics {
        output.push_str(&format!(
            "| {} | {}x{} | {}x{} | {:.4} | {:.2}% | {:.4} |\n",
            metric.case,
            metric.old_size[0],
            metric.old_size[1],
            metric.new_size[0],
            metric.new_size[1],
            metric.ssim,
            metric.changed_pixel_ratio * 100.0,
            metric.mean_absolute_error,
        ));
    }
    output.push_str("\n## Visual comparison\n\n| Case | Legacy | badge-maker-rs 0.2.0 | Difference |\n| --- | --- | --- | --- |\n");
    for metric in metrics {
        output.push_str(&format!(
            "| {} | ![legacy](./{}--old.png) | ![new](./{}--new.png) | ![difference](./{}--diff.png) |\n",
            metric.case, metric.case, metric.case, metric.case,
        ));
    }
    output
}
