#include "IconRasterizer.hpp"

#include <pango/pangocairo.h>

constexpr double GLYPH_RATIO      = 0.75;
constexpr auto   GLYPH_FAMILY     = "Material Symbols Rounded";
constexpr auto   GLYPH_VARIATIONS = "FILL=1,opsz=20,wght=600";

CairoSurfacePtr  rasterizeIcon(const std::string& glyphName, const SIconColor& color, int surfaceSize) {
    CairoSurfacePtr surface{cairo_image_surface_create(CAIRO_FORMAT_ARGB32, surfaceSize, surfaceSize)};
    auto*           cairo       = cairo_create(surface.get());
    auto*           layout      = pango_cairo_create_layout(cairo);
    auto*           font        = pango_font_description_new();
    auto*           fontOptions = cairo_font_options_create();

    cairo_font_options_set_hint_style(fontOptions, CAIRO_HINT_STYLE_NONE);
    cairo_font_options_set_hint_metrics(fontOptions, CAIRO_HINT_METRICS_OFF);
    cairo_font_options_set_antialias(fontOptions, CAIRO_ANTIALIAS_GRAY);

    auto* context = pango_layout_get_context(layout);
    pango_cairo_context_set_font_options(context, fontOptions);
    pango_context_set_round_glyph_positions(context, false);

    pango_font_description_set_family_static(font, GLYPH_FAMILY);
    pango_font_description_set_absolute_size(font, surfaceSize * GLYPH_RATIO * PANGO_SCALE);
    pango_font_description_set_variations_static(font, GLYPH_VARIATIONS);

    pango_layout_set_font_description(layout, font);
    pango_layout_set_text(layout, glyphName.c_str(), -1);

    PangoRectangle inkBox = {};
    pango_layout_get_extents(layout, &inkBox, nullptr);

    const double UNIT  = PANGO_SCALE;
    const double INK_X = (surfaceSize - inkBox.width / UNIT) / 2.0 - inkBox.x / UNIT;
    const double INK_Y = (surfaceSize - inkBox.height / UNIT) / 2.0 - inkBox.y / UNIT;

    if (inkBox.width / UNIT <= surfaceSize && inkBox.height / UNIT <= surfaceSize) {
        cairo_set_source_rgba(cairo, color.red, color.green, color.blue, color.alpha);
        cairo_move_to(cairo, INK_X, INK_Y);
        pango_cairo_show_layout(cairo, layout);
    }

    cairo_font_options_destroy(fontOptions);
    pango_font_description_free(font);
    g_object_unref(layout);
    cairo_destroy(cairo);

    cairo_surface_flush(surface.get());

    return surface;
}
