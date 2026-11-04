#include "IconRasterizer.hpp"

constexpr double ICON_DESIGN_SIZE    = 16;
constexpr double ICON_STROKE_WIDTH   = 2;
constexpr double ICON_HALF_EXTENT    = 4;
constexpr double CHEVRON_HALF_HEIGHT = 2;

CairoSurfacePtr  rasterizeIcon(eBarIcon icon, const SIconColor& color, int surfaceSize) {
    CairoSurfacePtr surface{cairo_image_surface_create(CAIRO_FORMAT_ARGB32, surfaceSize, surfaceSize)};
    auto*           cairo = cairo_create(surface.get());

    cairo_translate(cairo, surfaceSize / 2.0, surfaceSize / 2.0);
    cairo_scale(cairo, surfaceSize / ICON_DESIGN_SIZE, surfaceSize / ICON_DESIGN_SIZE);

    cairo_set_source_rgba(cairo, color.red, color.green, color.blue, color.alpha);
    cairo_set_line_width(cairo, ICON_STROKE_WIDTH);
    cairo_set_line_cap(cairo, CAIRO_LINE_CAP_ROUND);
    cairo_set_line_join(cairo, CAIRO_LINE_JOIN_ROUND);

    switch (icon) {
        case BAR_ICON_MINIMIZE:
            cairo_move_to(cairo, -ICON_HALF_EXTENT, -CHEVRON_HALF_HEIGHT);
            cairo_line_to(cairo, 0, CHEVRON_HALF_HEIGHT);
            cairo_line_to(cairo, ICON_HALF_EXTENT, -CHEVRON_HALF_HEIGHT);
            break;
        case BAR_ICON_MAXIMIZE:
            cairo_move_to(cairo, -ICON_HALF_EXTENT, CHEVRON_HALF_HEIGHT);
            cairo_line_to(cairo, 0, -CHEVRON_HALF_HEIGHT);
            cairo_line_to(cairo, ICON_HALF_EXTENT, CHEVRON_HALF_HEIGHT);
            break;
        case BAR_ICON_RESTORE:
            cairo_move_to(cairo, -ICON_HALF_EXTENT, 0);
            cairo_line_to(cairo, 0, -ICON_HALF_EXTENT);
            cairo_line_to(cairo, ICON_HALF_EXTENT, 0);
            cairo_line_to(cairo, 0, ICON_HALF_EXTENT);
            cairo_close_path(cairo);
            break;
        case BAR_ICON_CLOSE:
            cairo_move_to(cairo, -ICON_HALF_EXTENT, -ICON_HALF_EXTENT);
            cairo_line_to(cairo, ICON_HALF_EXTENT, ICON_HALF_EXTENT);
            cairo_move_to(cairo, ICON_HALF_EXTENT, -ICON_HALF_EXTENT);
            cairo_line_to(cairo, -ICON_HALF_EXTENT, ICON_HALF_EXTENT);
            break;
    }

    cairo_stroke(cairo);
    cairo_destroy(cairo);

    cairo_surface_flush(surface.get());

    return surface;
}
