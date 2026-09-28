use gtk4::gdk;
use gtk4::gdk_pixbuf::{InterpType, Pixbuf};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::path::PathBuf;

type Pixels = (i32, i32, usize, bool, glib::Bytes);

const BLUR_PIXELS_PER_SIGMA: f64 = 3.0;
const BLUR_MAX_SHRINK: f64 = 8.0;
const BLUR_PASSES: usize = 3;

pub async fn texture(path: PathBuf, (width, height): (i32, i32)) -> Option<gdk::Texture> {
    let pixels = gio::spawn_blocking(move || {
        let pixbuf = Pixbuf::from_file_at_scale(&path, width, height, true).ok()?;
        Some(pixels(&pixbuf))
    })
    .await
    .ok()
    .flatten()?;
    Some(upload(pixels))
}

pub async fn cover_textures<T: Send + 'static>(
    path: PathBuf,
    sizes: Vec<(i32, i32)>,
    measure: impl FnOnce(&Pixbuf) -> T + Send + 'static,
) -> Option<(Vec<gdk::Texture>, T)> {
    let (covers, measured) = gio::spawn_blocking(move || {
        let pixbuf = Pixbuf::from_file(&path).ok()?;
        let covers = sizes
            .into_iter()
            .map(|size| cover(&pixbuf, size).map(|scaled| pixels(&scaled)))
            .collect::<Option<Vec<_>>>()?;
        Some((covers, measure(&pixbuf)))
    })
    .await
    .ok()
    .flatten()?;
    Some((covers.into_iter().map(upload).collect(), measured))
}

pub async fn blurred_texture(path: PathBuf, size: (i32, i32), sigma: f64) -> Option<gdk::Texture> {
    let pixels = gio::spawn_blocking(move || {
        let shrink = (sigma / BLUR_PIXELS_PER_SIGMA)
            .floor()
            .clamp(1.0, BLUR_MAX_SHRINK);
        let width = ((size.0 as f64 / shrink).round() as i32).max(1);
        let height = ((size.1 as f64 / shrink).round() as i32).max(1);
        let pixbuf = Pixbuf::from_file_at_scale(&path, width, height, false).ok()?;
        let (width, height, stride, alpha, bytes) = pixels(&pixbuf);
        let mut data = bytes.to_vec();
        let channels = if alpha { 4 } else { 3 };
        gaussian_blur(
            &mut data,
            (width as usize, height as usize),
            stride,
            channels,
            sigma / shrink,
        );
        Some((width, height, stride, alpha, glib::Bytes::from_owned(data)))
    })
    .await
    .ok()
    .flatten()?;
    Some(upload(pixels))
}

fn box_sizes(sigma: f64) -> [usize; BLUR_PASSES] {
    let passes = BLUR_PASSES as f64;
    let ideal = (12.0 * sigma * sigma / passes + 1.0).sqrt();
    let mut lower = ideal.floor();
    if lower % 2.0 == 0.0 {
        lower -= 1.0;
    }
    let lower_count =
        ((12.0 * sigma * sigma - passes * lower * lower - 4.0 * passes * lower - 3.0 * passes)
            / (-4.0 * lower - 4.0))
            .round()
            .max(0.0) as usize;
    std::array::from_fn(|pass| {
        let size = if pass < lower_count {
            lower
        } else {
            lower + 2.0
        };
        size.max(1.0) as usize
    })
}

pub fn gaussian_blur(
    data: &mut [u8],
    (width, height): (usize, usize),
    stride: usize,
    channels: usize,
    sigma: f64,
) {
    if width == 0 || height == 0 || sigma <= 0.0 {
        return;
    }
    let mut line = Vec::new();
    for size in box_sizes(sigma) {
        let radius = (size - 1) / 2;
        if radius == 0 {
            continue;
        }
        for y in 0..height {
            box_line(
                data,
                (y * stride, channels),
                width,
                channels,
                radius,
                &mut line,
            );
        }
        for x in 0..width {
            box_line(
                data,
                (x * channels, stride),
                height,
                channels,
                radius,
                &mut line,
            );
        }
    }
}

fn box_line(
    data: &mut [u8],
    (start, step): (usize, usize),
    count: usize,
    channels: usize,
    radius: usize,
    line: &mut Vec<u32>,
) {
    let window = (2 * radius + 1) as u32;
    let last = count - 1;
    for channel in 0..channels {
        line.clear();
        line.extend((0..count).map(|index| data[start + index * step + channel] as u32));
        let mut sum = line[0] * (radius as u32 + 1)
            + (1..=radius).map(|index| line[index.min(last)]).sum::<u32>();
        for index in 0..count {
            data[start + index * step + channel] = ((sum + window / 2) / window) as u8;
            sum += line[(index + radius + 1).min(last)];
            sum -= line[index.saturating_sub(radius)];
        }
    }
}

pub async fn cover_texture(path: PathBuf, size: (i32, i32)) -> Option<gdk::Texture> {
    let pixels = gio::spawn_blocking(move || {
        let pixbuf = Pixbuf::from_file(&path).ok()?;
        Some(pixels(&cover(&pixbuf, size)?))
    })
    .await
    .ok()
    .flatten()?;
    Some(upload(pixels))
}

fn cover(pixbuf: &Pixbuf, (width, height): (i32, i32)) -> Option<Pixbuf> {
    let scale = (width as f64 / pixbuf.width() as f64).max(height as f64 / pixbuf.height() as f64);
    let crop_width = ((width as f64 / scale).round() as i32).min(pixbuf.width());
    let crop_height = ((height as f64 / scale).round() as i32).min(pixbuf.height());
    let cropped = pixbuf.new_subpixbuf(
        (pixbuf.width() - crop_width) / 2,
        (pixbuf.height() - crop_height) / 2,
        crop_width,
        crop_height,
    );
    cropped.scale_simple(width, height, InterpType::Bilinear)
}

fn pixels(pixbuf: &Pixbuf) -> Pixels {
    (
        pixbuf.width(),
        pixbuf.height(),
        pixbuf.rowstride() as usize,
        pixbuf.has_alpha(),
        pixbuf.read_pixel_bytes(),
    )
}

fn upload((width, height, stride, alpha, pixels): Pixels) -> gdk::Texture {
    let format = if alpha {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    gdk::MemoryTexture::new(width, height, format, &pixels, stride).upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_boxes_add_up_to_the_gaussian_variance() {
        for sigma in [2.0, 5.6, 14.0] {
            let variance: f64 = box_sizes(sigma)
                .iter()
                .map(|&size| ((size * size) as f64 - 1.0) / 12.0)
                .sum();
            assert!(
                (variance.sqrt() - sigma).abs() < sigma * 0.1,
                "{sigma}: {variance}"
            );
        }
    }

    #[test]
    fn blur_keeps_a_flat_image_and_spreads_a_point_evenly() {
        let (width, height) = (21, 21);
        let mut flat = vec![90u8; width * height * 3];
        gaussian_blur(&mut flat, (width, height), width * 3, 3, 3.0);
        assert!(flat.iter().all(|&value| value == 90));

        let mut spread = vec![0u8; width * height];
        spread[10 * width + 10] = 255;
        gaussian_blur(&mut spread, (width, height), width, 1, 1.5);
        let at = |x: usize, y: usize| spread[y * width + x];
        assert!(at(10, 10) > at(11, 10) && at(11, 10) > at(12, 10));
        assert_eq!(at(9, 10), at(11, 10));
        assert_eq!(at(10, 9), at(10, 11));
        assert_eq!(at(9, 10), at(10, 9));
    }
}
