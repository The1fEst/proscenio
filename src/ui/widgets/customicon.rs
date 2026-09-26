use gtk4::cairo;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

pub fn build(name: &str, size: i32) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.add_css_class("custom-icon");
    area.set_content_width(size);
    area.set_content_height(size);
    let svg = crate::core::assets::icon(name);
    let cache: Rc<RefCell<Option<(i32, cairo::ImageSurface)>>> = Rc::new(RefCell::new(None));
    area.set_draw_func(move |area, cr, width, height| {
        let Some(svg) = svg else {
            return;
        };
        let scale = area.scale_factor().max(1);
        let pixels = width.min(height) * scale;
        let cached = cache
            .borrow()
            .as_ref()
            .filter(|(size, _)| *size == pixels)
            .map(|(_, surface)| surface.clone());
        let surface = match cached {
            Some(surface) => surface,
            None => {
                let Some(surface) = rasterise(svg, pixels) else {
                    return;
                };
                cache.replace(Some((pixels, surface.clone())));
                surface
            }
        };
        let colour = area.color();
        let _ = cr.save();
        cr.scale(1.0 / scale as f64, 1.0 / scale as f64);
        cr.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            colour.alpha() as f64,
        );
        let _ = cr.mask_surface(&surface, 0.0, 0.0);
        let _ = cr.restore();
    });
    area
}

fn rasterise(svg: &'static [u8], size: i32) -> Option<cairo::ImageSurface> {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(svg));
    let pixbuf =
        Pixbuf::from_stream_at_scale(&stream, size, size, true, gio::Cancellable::NONE).ok()?;
    let (width, height) = (pixbuf.width(), pixbuf.height());
    let channels = pixbuf.n_channels() as usize;
    let source_stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let from = y * source_stride + x * channels;
                let alpha = if channels == 4 { bytes[from + 3] } else { 255 };
                let to = y * stride + x * 4;
                data[to..to + 4].copy_from_slice(&[0, 0, 0, alpha]);
            }
        }
    }
    surface.mark_dirty();
    Some(surface)
}
