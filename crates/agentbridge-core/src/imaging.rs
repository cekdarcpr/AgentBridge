//! Turns a provider's raw image into the asset that was asked for: exact size, requested format,
//! transparent background and trimmed margins. Runs locally, so it costs nothing.

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    str::FromStr,
};

use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder},
    imageops::{self, FilterType},
    ExtendedColorType, ImageEncoder, ImageFormat as RawFormat, Rgba, RgbaImage,
};
use serde::Serialize;

use crate::{error::AppError, request::ImageFormat};

/// Appended to prompts for providers that cannot return transparency: a flat, unusual backdrop
/// is easy to remove reliably afterwards.
pub const BACKDROP_PROMPT: &str = "Isolated on a solid, flat, pure magenta (#FF00FF) background. \
No shadows, no gradients, no scenery and nothing else in the background.";

/// RGB distance up to which a pixel counts as background.
const HARD_TOLERANCE: f32 = 48.0;
/// Pixels this close to the removed area may be anti-aliased blends of subject and backdrop.
const EDGE_RADIUS: i64 = 2;
/// How far to look for the pure subject color an edge pixel was blended from.
const SUBJECT_SEARCH_RADIUS: i64 = 3;
/// A subject color closer than this to the backdrop gives no reliable blend estimate.
const MIN_SUBJECT_CONTRAST: f32 = 60.0;
/// Edge pixels with at least this much subject are left untouched.
const OPAQUE_ALPHA: f32 = 0.98;
/// Share of border pixels that must match the backdrop color before anything is removed.
const MIN_UNIFORM_BORDER: f32 = 0.6;
/// Edge alpha below this is treated as fully transparent; un-premultiplying it would amplify noise.
const MIN_EDGE_ALPHA: f32 = 0.05;
/// Border colors are grouped by their top bits to find the dominant one.
const QUANTIZE_SHIFT: u8 = 4;
/// Pixels at or below this alpha are empty for trimming purposes.
const TRIM_ALPHA: u8 = 8;
const JPEG_QUALITY: u8 = 90;
const UPSCALE_EPSILON: f32 = 0.01;
pub const MAX_DIMENSION: u32 = 8192;

const TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);
const WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    /// Fill the whole frame, cropping the overflow from the center.
    Cover,
    /// Fit the whole image inside the frame and pad the rest.
    Contain,
    /// Scale to the exact size, ignoring the aspect ratio.
    Stretch,
}

impl Fit {
    /// Transparent assets are usually icons or sprites, where cropping would cut off the subject.
    pub fn default_for(transparent: bool) -> Self {
        if transparent {
            Self::Contain
        } else {
            Self::Cover
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
            Self::Stretch => "stretch",
        }
    }
}

impl fmt::Display for Fit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Fit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "cover" => Ok(Self::Cover),
            "contain" => Ok(Self::Contain),
            "stretch" => Ok(Self::Stretch),
            other => Err(format!(
                "unknown fit '{other}' (expected: cover, contain, stretch)"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ImageSpec {
    pub size: Option<(u32, u32)>,
    pub fit: Fit,
    pub remove_background: bool,
    pub trim: bool,
    pub format: ImageFormat,
}

#[derive(Debug)]
pub struct Processed {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub warnings: Vec<String>,
}

pub fn decode(data: &[u8]) -> Result<RgbaImage, AppError> {
    image::load_from_memory(data)
        .map(|img| img.to_rgba8())
        .map_err(|e| AppError::Output(format!("could not decode the image: {e}")))
}

pub fn process(data: &[u8], spec: &ImageSpec) -> Result<Processed, AppError> {
    let source_format = image::guess_format(data).ok();
    let mut img = decode(data)?;
    let mut warnings = Vec::new();

    let needs_work = spec.remove_background
        || spec.trim
        || spec.size.is_some_and(|size| size != img.dimensions())
        || source_format != Some(raw_format(spec.format));
    if !needs_work {
        let (width, height) = img.dimensions();
        return Ok(Processed {
            data: data.to_vec(),
            width,
            height,
            warnings,
        });
    }

    if spec.remove_background && !remove_background(&mut img) {
        warnings.push(
            "the background is not one flat color, so it could not be made transparent".into(),
        );
    }
    if spec.trim {
        match trim(&img) {
            Some(trimmed) => img = trimmed,
            None => {
                warnings.push("--trim had no effect: the image has no transparent margin".into())
            }
        }
    }
    if let Some((width, height)) = spec.size {
        let (source_w, source_h) = img.dimensions();
        let scale = scale_factor(source_w, source_h, width, height, spec.fit);
        if scale > 1.0 + UPSCALE_EPSILON {
            warnings.push(format!(
                "upscaled from {source_w}x{source_h} to {width}x{height}; fine detail may be soft"
            ));
        }
        img = fit(&img, width, height, spec.fit, spec.format.supports_alpha());
    }

    let (width, height) = img.dimensions();
    let data = encode(&img, spec.format)?;
    Ok(Processed {
        data,
        width,
        height,
        warnings,
    })
}

fn raw_format(format: ImageFormat) -> RawFormat {
    match format {
        ImageFormat::Png => RawFormat::Png,
        ImageFormat::Jpeg => RawFormat::Jpeg,
        ImageFormat::Webp => RawFormat::WebP,
    }
}

fn scale_factor(source_w: u32, source_h: u32, width: u32, height: u32, fit: Fit) -> f32 {
    let sx = width as f32 / source_w as f32;
    let sy = height as f32 / source_h as f32;
    match fit {
        Fit::Cover => sx.max(sy),
        Fit::Contain => sx.min(sy),
        Fit::Stretch => sx.max(sy),
    }
}

pub fn fit(img: &RgbaImage, width: u32, height: u32, fit: Fit, keep_alpha: bool) -> RgbaImage {
    let (source_w, source_h) = img.dimensions();
    match fit {
        Fit::Stretch => imageops::resize(img, width, height, FilterType::Lanczos3),
        Fit::Cover => {
            let scale = scale_factor(source_w, source_h, width, height, Fit::Cover);
            let scaled_w = ((source_w as f32 * scale).round() as u32).max(width);
            let scaled_h = ((source_h as f32 * scale).round() as u32).max(height);
            let resized = imageops::resize(img, scaled_w, scaled_h, FilterType::Lanczos3);
            let x = (scaled_w - width) / 2;
            let y = (scaled_h - height) / 2;
            imageops::crop_imm(&resized, x, y, width, height).to_image()
        }
        Fit::Contain => {
            let scale = scale_factor(source_w, source_h, width, height, Fit::Contain);
            let scaled_w = ((source_w as f32 * scale).round() as u32).clamp(1, width);
            let scaled_h = ((source_h as f32 * scale).round() as u32).clamp(1, height);
            let resized = imageops::resize(img, scaled_w, scaled_h, FilterType::Lanczos3);
            let mut canvas =
                RgbaImage::from_pixel(width, height, if keep_alpha { TRANSPARENT } else { WHITE });
            let x = i64::from((width - scaled_w) / 2);
            let y = i64::from((height - scaled_h) / 2);
            imageops::overlay(&mut canvas, &resized, x, y);
            canvas
        }
    }
}

pub fn encode(img: &RgbaImage, format: ImageFormat) -> Result<Vec<u8>, AppError> {
    let (width, height) = img.dimensions();
    let mut out = Vec::new();
    let result = match format {
        ImageFormat::Png => PngEncoder::new(&mut out).write_image(
            img.as_raw(),
            width,
            height,
            ExtendedColorType::Rgba8,
        ),
        ImageFormat::Webp => WebPEncoder::new_lossless(&mut out).write_image(
            img.as_raw(),
            width,
            height,
            ExtendedColorType::Rgba8,
        ),
        ImageFormat::Jpeg => {
            let flat = flatten(img, WHITE);
            JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).write_image(
                flat.as_raw(),
                width,
                height,
                ExtendedColorType::Rgb8,
            )
        }
    };
    result.map_err(|e| {
        AppError::Output(format!(
            "could not encode the image as {}: {e}",
            format.as_str()
        ))
    })?;
    Ok(out)
}

/// Composites onto a solid color, for formats and platforms without transparency.
pub fn flatten(img: &RgbaImage, background: Rgba<u8>) -> image::RgbImage {
    image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let Rgba([r, g, b, a]) = *img.get_pixel(x, y);
        let alpha = f32::from(a) / 255.0;
        let blend =
            |c: u8, bg: u8| (f32::from(c) * alpha + f32::from(bg) * (1.0 - alpha)).round() as u8;
        image::Rgb([
            blend(r, background[0]),
            blend(g, background[1]),
            blend(b, background[2]),
        ])
    })
}

/// Removes a flat background connected to the image border. Returns false when the border is
/// not a single dominant color, in which case the image is left untouched.
pub fn remove_background(img: &mut RgbaImage) -> bool {
    let (width, height) = img.dimensions();
    let border = border_pixels(width, height);
    let transparent_border = border
        .iter()
        .filter(|&&(x, y)| img.get_pixel(x, y)[3] <= TRIM_ALPHA)
        .count();
    if transparent_border as f32 / border.len() as f32 >= MIN_UNIFORM_BORDER {
        return true;
    }
    let Some(backdrop) = dominant_color(img, &border) else {
        return false;
    };

    // Flood fill from the border so that backdrop-colored pixels inside the subject survive.
    let index = |x: u32, y: u32| (y * width + x) as usize;
    let mut removed = vec![false; (width * height) as usize];
    let mut queue = VecDeque::new();
    for &(x, y) in &border {
        if !removed[index(x, y)] && distance(img.get_pixel(x, y), backdrop) <= HARD_TOLERANCE {
            removed[index(x, y)] = true;
            queue.push_back((x, y));
        }
    }
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in neighbors(x, y, width, height) {
            if !removed[index(nx, ny)]
                && distance(img.get_pixel(nx, ny), backdrop) <= HARD_TOLERANCE
            {
                removed[index(nx, ny)] = true;
                queue.push_back((nx, ny));
            }
        }
    }

    // Edge pixels are judged against the untouched source, not against already-cleaned neighbours.
    let source = img.clone();
    let near_removed = |x: u32, y: u32, radius: i64| {
        window(x, y, radius, width, height).any(|(nx, ny)| removed[index(nx, ny)])
    };
    for y in 0..height {
        for x in 0..width {
            if removed[index(x, y)] {
                img.put_pixel(x, y, TRANSPARENT);
            } else if near_removed(x, y, EDGE_RADIUS) {
                let subject = window(x, y, SUBJECT_SEARCH_RADIUS, width, height)
                    .filter(|&(nx, ny)| !removed[index(nx, ny)])
                    .map(|(nx, ny)| *source.get_pixel(nx, ny))
                    .max_by(|a, b| distance(a, backdrop).total_cmp(&distance(b, backdrop)));
                if let Some(subject) = subject {
                    unblend_edge(img, x, y, subject, backdrop);
                }
            }
        }
    }
    true
}

/// Treats an anti-aliased edge pixel as a blend of `subject` and `backdrop`: the blend ratio
/// becomes its alpha and the backdrop's share is removed from its color, so no fringe remains.
fn unblend_edge(img: &mut RgbaImage, x: u32, y: u32, subject: Rgba<u8>, backdrop: [f32; 3]) {
    let pixel = *img.get_pixel(x, y);
    let toward_subject: [f32; 3] = std::array::from_fn(|i| f32::from(subject[i]) - backdrop[i]);
    let toward_pixel: [f32; 3] = std::array::from_fn(|i| f32::from(pixel[i]) - backdrop[i]);
    let length_sq: f32 = toward_subject.iter().map(|v| v * v).sum();
    if length_sq.sqrt() < MIN_SUBJECT_CONTRAST {
        return;
    }
    let projection: f32 = toward_subject
        .iter()
        .zip(toward_pixel)
        .map(|(s, p)| s * p)
        .sum();
    let alpha = (projection / length_sq).clamp(0.0, 1.0);
    if alpha >= OPAQUE_ALPHA {
        return;
    }
    if alpha < MIN_EDGE_ALPHA {
        img.put_pixel(x, y, TRANSPARENT);
        return;
    }
    let unmix = |c: u8, bg: f32| {
        ((f32::from(c) - (1.0 - alpha) * bg) / alpha)
            .clamp(0.0, 255.0)
            .round() as u8
    };
    let new_alpha = (alpha * f32::from(pixel[3])).round() as u8;
    img.put_pixel(
        x,
        y,
        Rgba([
            unmix(pixel[0], backdrop[0]),
            unmix(pixel[1], backdrop[1]),
            unmix(pixel[2], backdrop[2]),
            new_alpha,
        ]),
    );
}

/// Pixels in the square of the given radius around (x, y), clipped to the image.
fn window(
    x: u32,
    y: u32,
    radius: i64,
    width: u32,
    height: u32,
) -> impl Iterator<Item = (u32, u32)> {
    let (cx, cy) = (i64::from(x), i64::from(y));
    let (max_x, max_y) = (i64::from(width) - 1, i64::from(height) - 1);
    let xs = (cx - radius).max(0)..=(cx + radius).min(max_x);
    let ys = (cy - radius).max(0)..=(cy + radius).min(max_y);
    ys.flat_map(move |ny| xs.clone().map(move |nx| (nx as u32, ny as u32)))
}

fn dominant_color(img: &RgbaImage, border: &[(u32, u32)]) -> Option<[f32; 3]> {
    let mut buckets: HashMap<[u8; 3], (u32, [u64; 3])> = HashMap::new();
    for &(x, y) in border {
        let Rgba([r, g, b, _]) = *img.get_pixel(x, y);
        let entry = buckets
            .entry([
                r >> QUANTIZE_SHIFT,
                g >> QUANTIZE_SHIFT,
                b >> QUANTIZE_SHIFT,
            ])
            .or_default();
        entry.0 += 1;
        entry.1[0] += u64::from(r);
        entry.1[1] += u64::from(g);
        entry.1[2] += u64::from(b);
    }
    let (count, sums) = buckets.into_values().max_by_key(|(count, _)| *count)?;
    let average = sums.map(|sum| sum as f32 / count as f32);
    let matching = border
        .iter()
        .filter(|&&(x, y)| distance(img.get_pixel(x, y), average) <= HARD_TOLERANCE)
        .count();
    (matching as f32 / border.len() as f32 >= MIN_UNIFORM_BORDER).then_some(average)
}

fn distance(pixel: &Rgba<u8>, color: [f32; 3]) -> f32 {
    let d = |i: usize| f32::from(pixel[i]) - color[i];
    (d(0) * d(0) + d(1) * d(1) + d(2) * d(2)).sqrt()
}

fn border_pixels(width: u32, height: u32) -> Vec<(u32, u32)> {
    let mut pixels: Vec<(u32, u32)> = (0..width).flat_map(|x| [(x, 0), (x, height - 1)]).collect();
    pixels.extend((1..height.saturating_sub(1)).flat_map(|y| [(0, y), (width - 1, y)]));
    pixels
}

fn neighbors(x: u32, y: u32, width: u32, height: u32) -> impl Iterator<Item = (u32, u32)> {
    let candidates = [
        (x.checked_sub(1), Some(y)),
        ((x + 1 < width).then_some(x + 1), Some(y)),
        (Some(x), y.checked_sub(1)),
        (Some(x), (y + 1 < height).then_some(y + 1)),
    ];
    candidates
        .into_iter()
        .filter_map(|(nx, ny)| Some((nx?, ny?)))
}

/// Crops to the bounding box of visible pixels; `None` when there is nothing to remove.
pub fn trim(img: &RgbaImage) -> Option<RgbaImage> {
    let (width, height) = img.dimensions();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0, 0);
    for (x, y, pixel) in img.enumerate_pixels() {
        if pixel[3] > TRIM_ALPHA {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x || (min_x == 0 && min_y == 0 && max_x == width - 1 && max_y == height - 1) {
        return None;
    }
    Some(imageops::crop_imm(img, min_x, min_y, max_x - min_x + 1, max_y - min_y + 1).to_image())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAGENTA: Rgba<u8> = Rgba([255, 0, 255, 255]);
    const BLUE: Rgba<u8> = Rgba([20, 40, 200, 255]);

    /// A blue square in the middle of a magenta backdrop, like a keyed provider output.
    fn sprite(size: u32, inset: u32) -> RgbaImage {
        RgbaImage::from_fn(size, size, |x, y| {
            let inside = (inset..size - inset).contains(&x) && (inset..size - inset).contains(&y);
            if inside {
                BLUE
            } else {
                MAGENTA
            }
        })
    }

    fn png(img: &RgbaImage) -> Vec<u8> {
        encode(img, ImageFormat::Png).unwrap()
    }

    fn spec(format: ImageFormat) -> ImageSpec {
        ImageSpec {
            size: None,
            fit: Fit::Cover,
            remove_background: false,
            trim: false,
            format,
        }
    }

    #[test]
    fn removes_flat_backdrop_and_keeps_subject() {
        let mut img = sprite(40, 10);
        assert!(remove_background(&mut img));
        assert_eq!(img.get_pixel(0, 0)[3], 0);
        assert_eq!(*img.get_pixel(20, 20), BLUE);
    }

    #[test]
    fn keeps_backdrop_colored_pixels_inside_the_subject() {
        let mut img = sprite(40, 10);
        img.put_pixel(20, 20, MAGENTA);
        assert!(remove_background(&mut img));
        assert_eq!(img.get_pixel(20, 20)[3], 255);
    }

    #[test]
    fn blended_edges_lose_the_backdrop_tint() {
        // A yellow box on magenta whose left edge column is a 50/50 blend of the two.
        let yellow = Rgba([250, 200, 40, 255]);
        let mut img = RgbaImage::from_fn(40, 30, |x, y| {
            let rows = (5..25).contains(&y);
            match x {
                20 if rows => Rgba([253, 100, 148, 255]),
                21..=34 if rows => yellow,
                _ => MAGENTA,
            }
        });
        assert!(remove_background(&mut img));
        let edge = img.get_pixel(20, 15);
        assert!(
            (100..=155).contains(&edge[3]),
            "blend becomes half transparent: {edge:?}"
        );
        assert!(
            edge[1] > 180 && edge[2] < 80,
            "magenta tint removed: {edge:?}"
        );
        assert_eq!(*img.get_pixel(28, 15), yellow);
    }

    #[test]
    fn refuses_busy_backgrounds() {
        let mut img = RgbaImage::from_fn(30, 30, |x, y| {
            Rgba([(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8, 255])
        });
        let before = img.clone();
        assert!(!remove_background(&mut img));
        assert_eq!(img, before);
    }

    #[test]
    fn trims_transparent_margins() {
        let mut img = sprite(40, 10);
        remove_background(&mut img);
        let trimmed = trim(&img).unwrap();
        assert_eq!(trimmed.dimensions(), (20, 20));
        assert!(trim(&sprite(10, 2)).is_none());
    }

    #[test]
    fn fits_exact_sizes() {
        let wide = RgbaImage::from_pixel(200, 100, BLUE);
        assert_eq!(fit(&wide, 64, 64, Fit::Cover, true).dimensions(), (64, 64));
        let contained = fit(&wide, 64, 64, Fit::Contain, true);
        assert_eq!(contained.dimensions(), (64, 64));
        assert_eq!(
            contained.get_pixel(32, 0)[3],
            0,
            "contain pads with transparency"
        );
        assert_eq!(contained.get_pixel(32, 32)[3], 255);
        assert_eq!(
            fit(&wide, 30, 90, Fit::Stretch, true).dimensions(),
            (30, 90)
        );
    }

    #[test]
    fn processes_to_requested_size_and_format() {
        let source = png(&sprite(100, 25));
        let spec = ImageSpec {
            size: Some((32, 32)),
            fit: Fit::Contain,
            remove_background: true,
            trim: false,
            format: ImageFormat::Webp,
        };
        let out = process(&source, &spec).unwrap();
        assert_eq!((out.width, out.height), (32, 32));
        assert_eq!(image::guess_format(&out.data).unwrap(), RawFormat::WebP);
        assert!(out.warnings.is_empty(), "{:?}", out.warnings);
        assert_eq!(
            decode(&out.data).unwrap().get_pixel(0, 0)[3],
            0,
            "backdrop became transparent"
        );

        // Trimming first drops the margin, so the subject fills the frame.
        let trimmed = process(&source, &ImageSpec { trim: true, ..spec }).unwrap();
        assert_eq!(decode(&trimmed.data).unwrap().get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn jpeg_output_is_flattened_on_white() {
        let mut img = sprite(20, 5);
        remove_background(&mut img);
        let out = process(&png(&img), &spec(ImageFormat::Jpeg)).unwrap();
        assert_eq!(image::guess_format(&out.data).unwrap(), RawFormat::Jpeg);
        let corner = decode(&out.data).unwrap().get_pixel(0, 0).0;
        assert!(
            corner[0] > 240 && corner[1] > 240 && corner[2] > 240,
            "{corner:?}"
        );
    }

    #[test]
    fn untouched_images_keep_their_bytes() {
        let source = png(&sprite(16, 4));
        assert_eq!(
            process(&source, &spec(ImageFormat::Png)).unwrap().data,
            source
        );
    }

    #[test]
    fn warns_when_upscaling() {
        let source = png(&RgbaImage::from_pixel(10, 10, BLUE));
        let spec = ImageSpec {
            size: Some((40, 40)),
            ..spec(ImageFormat::Png)
        };
        assert!(process(&source, &spec).unwrap().warnings[0].contains("upscaled"));
    }
}
