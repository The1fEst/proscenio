use gtk4::gdk::{self, RGBA};
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};

use super::{APP_ICON_SHARE, GAPS_OUT};
use crate::ui::anim::Tween;
use crate::ui::theme::{Adapted, rounding, transparentize};

const RADIUS: f32 = (rounding::SCREEN_ROUNDING - GAPS_OUT + 1) as f32;
const ART_RADIUS: f32 = rounding::VERYSMALL as f32;
const ART_BLUR: f32 = 70.0;
const WAVE_BLUR: f32 = 7.0;
const SATURATION: f32 = 0.2;
const SCRIM: f32 = 0.3;
const WAVE_ALPHA: f32 = 0.15;
const VISUALIZER_MAX: f64 = 1000.0;
const SMOOTHING: i64 = 2;

pub(super) struct Look {
    pub(super) adapted_colors: Cell<Adapted>,
    pub(super) texture: RefCell<Option<gdk::Texture>>,
    pub(super) thumbnail: RefCell<Option<gdk::Texture>>,
    pub(super) blur_cache: RefCell<Option<Blurred>>,
    pub(super) app_icon: RefCell<Option<gdk::Paintable>>,
    pub(super) opacity: Cell<Tween>,
    pub(super) is_playing: Cell<bool>,
}

pub(super) struct Blurred {
    source: gdk::Texture,
    width: f32,
    height: f32,
    scrim: RGBA,
    layer: gdk::Texture,
}

pub(super) fn paint_backdrop(
    snapshot: &gtk4::Snapshot,
    (width, height): (f32, f32),
    scale: i32,
    look: &Look,
    points: &[f64],
    renderer: Option<&gsk::Renderer>,
) {
    let bounds = graphene::Rect::new(0.0, 0.0, width, height);
    let adapted_colors = look.adapted_colors.get();
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, RADIUS));
    let mut base = adapted_colors.col_layer0;
    base.set_alpha(1.0);
    snapshot.append_color(&base, &bounds);

    let opacity = look
        .opacity
        .get()
        .value(glib::monotonic_time())
        .clamp(0.0, 1.0);
    if let Some(texture) = look.texture.borrow().as_ref()
        && opacity > 0.0
    {
        snapshot.push_opacity(opacity);
        let scrim = transparentize(adapted_colors.col_layer0, SCRIM);
        match render_blurred_art(look, texture, bounds, scrim, renderer) {
            Some(layer) => snapshot.append_texture(&layer, &bounds),
            None => blur_art(snapshot, texture, bounds, scrim),
        }
        snapshot.pop();
    }

    if look.is_playing.get()
        && points.len() >= 2
        && let Some((texture, area)) = blur_wave(
            (width, height),
            scale,
            points,
            saturate(adapted_colors.col_primary),
        )
    {
        snapshot.append_texture(&texture, &area);
    }
    snapshot.pop();
}

fn blur_wave(
    (width, height): (f32, f32),
    scale: i32,
    points: &[f64],
    color: RGBA,
) -> Option<(gdk::Texture, graphene::Rect)> {
    let scale = scale.max(1) as f64;
    let sigma = WAVE_BLUR as f64 / 2.0 * scale;
    let pad = (3.0 * sigma).ceil() as i32;
    let mask_width = (width as f64 * scale).ceil() as i32 + 2 * pad;
    let mask_height = (height as f64 * scale).ceil() as i32 + 2 * pad;
    let mut mask =
        gtk4::cairo::ImageSurface::create(gtk4::cairo::Format::A8, mask_width, mask_height).ok()?;
    {
        let cr = gtk4::cairo::Context::new(&mask).ok()?;
        cr.translate(pad as f64, pad as f64);
        cr.scale(scale, scale);
        wave(&cr, width as f64, height as f64, points);
        cr.fill().ok()?;
    }
    mask.flush();
    let stride = mask.stride() as usize;
    let mut coverage = mask.data().ok()?.to_vec();
    crate::ui::image::gaussian_blur(
        &mut coverage,
        (mask_width as usize, mask_height as usize),
        stride,
        1,
        sigma,
    );
    let tint = [color.red(), color.green(), color.blue(), 1.0];
    let mut pixels = Vec::with_capacity((mask_width * mask_height * 4) as usize);
    for row in coverage.chunks(stride).take(mask_height as usize) {
        for &value in &row[..mask_width as usize] {
            let alpha = value as f32 / 255.0 * WAVE_ALPHA;
            pixels.extend(tint.map(|channel| (channel * alpha * 255.0).round() as u8));
        }
    }
    let texture = gdk::MemoryTexture::new(
        mask_width,
        mask_height,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &glib::Bytes::from_owned(pixels),
        (mask_width * 4) as usize,
    );
    let area = graphene::Rect::new(
        (-pad as f64 / scale) as f32,
        (-pad as f64 / scale) as f32,
        (mask_width as f64 / scale) as f32,
        (mask_height as f64 / scale) as f32,
    );
    Some((texture.upcast(), area))
}

fn blur_art(
    snapshot: &gtk4::Snapshot,
    texture: &gdk::Texture,
    bounds: graphene::Rect,
    scrim: RGBA,
) {
    push_saturation(snapshot);
    snapshot.push_blur(ART_BLUR as f64);
    snapshot.push_clip(&bounds);
    snapshot.append_texture(texture, &cover(texture, bounds));
    snapshot.pop();
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, RADIUS));
    snapshot.append_color(&scrim, &bounds);
    snapshot.pop();
    snapshot.pop();
    snapshot.pop();
}

fn render_blurred_art(
    look: &Look,
    texture: &gdk::Texture,
    bounds: graphene::Rect,
    scrim: RGBA,
    renderer: Option<&gsk::Renderer>,
) -> Option<gdk::Texture> {
    let (width, height) = (bounds.width(), bounds.height());
    if let Some(cached) = look.blur_cache.borrow().as_ref()
        && cached.source == *texture
        && cached.width == width
        && cached.height == height
        && cached.scrim == scrim
    {
        return Some(cached.layer.clone());
    }
    let renderer = renderer?;
    let blur_snapshot = gtk4::Snapshot::new();
    blur_art(&blur_snapshot, texture, bounds, scrim);
    let layer = renderer.render_texture(blur_snapshot.to_node()?, Some(&bounds));
    look.blur_cache.replace(Some(Blurred {
        source: texture.clone(),
        width,
        height,
        scrim,
        layer: layer.clone(),
    }));
    Some(layer)
}

fn wave(cr: &gtk4::cairo::Context, width: f64, height: f64, points: &[f64]) {
    let count = points.len() as i64;
    cr.move_to(0.0, height);
    for index in 0..count {
        let mut sum = 0.0;
        for offset in -SMOOTHING..=SMOOTHING {
            sum += points[(index + offset).clamp(0, count - 1) as usize];
        }
        let average = sum / (SMOOTHING * 2 + 1) as f64;
        let x = index as f64 * width / (count - 1) as f64;
        cr.line_to(x, height - average / VISUALIZER_MAX * height);
    }
    cr.line_to(width, height);
    cr.close_path();
}

pub(super) fn cover_art(snapshot: &gtk4::Snapshot, width: f32, height: f32, look: &Look) {
    let bounds = graphene::Rect::new(0.0, 0.0, width, height);
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, ART_RADIUS));
    snapshot.append_color(
        &transparentize(look.adapted_colors.get().col_layer1, 0.5),
        &bounds,
    );
    let opacity = look
        .opacity
        .get()
        .value(glib::monotonic_time())
        .clamp(0.0, 1.0);
    if let Some(texture) = look.thumbnail.borrow().as_ref()
        && opacity > 0.0
    {
        snapshot.push_opacity(opacity);
        snapshot.append_texture(texture, &cover(texture, bounds));
        snapshot.pop();
    }
    if let Some(icon) = look.app_icon.borrow().as_ref() {
        let side = height * APP_ICON_SHARE;
        snapshot.save();
        snapshot.translate(&graphene::Point::new(
            ((width - side) / 2.0).round(),
            ((height - side) / 2.0).round(),
        ));
        icon.snapshot(snapshot, side as f64, side as f64);
        snapshot.restore();
    }
    snapshot.pop();
}

fn cover(texture: &gdk::Texture, bounds: graphene::Rect) -> graphene::Rect {
    let (image_width, image_height) = (texture.width() as f32, texture.height() as f32);
    let scale = (bounds.width() / image_width).max(bounds.height() / image_height);
    let (width, height) = (image_width * scale, image_height * scale);
    graphene::Rect::new(
        (bounds.width() - width) / 2.0,
        (bounds.height() - height) / 2.0,
        width,
        height,
    )
}

fn push_saturation(snapshot: &gtk4::Snapshot) {
    snapshot.push_color_matrix(&saturation(), &graphene::Vec4::zero());
}

fn saturate(color: RGBA) -> RGBA {
    let [red, green, blue, alpha] = saturation()
        .transform_vec4(&graphene::Vec4::new(
            color.red(),
            color.green(),
            color.blue(),
            color.alpha(),
        ))
        .to_float();
    RGBA::new(
        red.clamp(0.0, 1.0),
        green.clamp(0.0, 1.0),
        blue.clamp(0.0, 1.0),
        alpha,
    )
}

fn saturation() -> graphene::Matrix {
    let amount = 1.0 + SATURATION;
    let gray = [0.299, 0.587, 0.114].map(|weight| weight * (1.0 - amount));
    graphene::Matrix::from_float([
        gray[0] + amount,
        gray[0],
        gray[0],
        0.0,
        gray[1],
        gray[1] + amount,
        gray[1],
        0.0,
        gray[2],
        gray[2],
        gray[2] + amount,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saturating_a_color_keeps_grays_and_pushes_hues_away_from_gray() {
        let gray = saturate(RGBA::new(0.4, 0.4, 0.4, 0.7));
        assert!((gray.red() - 0.4).abs() < 1e-5 && (gray.blue() - 0.4).abs() < 1e-5);
        assert_eq!(gray.alpha(), 0.7);
        let purple = saturate(RGBA::new(0.6, 0.3, 0.7, 1.0));
        assert!(purple.red() > 0.6 && purple.green() < 0.3 && purple.blue() > 0.7);
    }
}
