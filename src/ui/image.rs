use gtk4::gdk;
use gtk4::gdk_pixbuf::{InterpType, Pixbuf};
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::path::PathBuf;

type Pixels = (i32, i32, usize, bool, glib::Bytes);

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

pub async fn cover_texture(path: PathBuf, (width, height): (i32, i32)) -> Option<gdk::Texture> {
    let pixels = gio::spawn_blocking(move || {
        let pixbuf = Pixbuf::from_file(&path).ok()?;
        let scale =
            (width as f64 / pixbuf.width() as f64).max(height as f64 / pixbuf.height() as f64);
        let crop_width = ((width as f64 / scale).round() as i32).min(pixbuf.width());
        let crop_height = ((height as f64 / scale).round() as i32).min(pixbuf.height());
        let cropped = pixbuf.new_subpixbuf(
            (pixbuf.width() - crop_width) / 2,
            (pixbuf.height() - crop_height) / 2,
            crop_width,
            crop_height,
        );
        let scaled = cropped.scale_simple(width, height, InterpType::Bilinear)?;
        Some(pixels(&scaled))
    })
    .await
    .ok()
    .flatten()?;
    Some(upload(pixels))
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
