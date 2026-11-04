#pragma once

#include "ButtonLayout.hpp"

#include <cairo/cairo.h>

#include <memory>

struct SIconColor {
    double red   = 0;
    double green = 0;
    double blue  = 0;
    double alpha = 1;
};

struct SCairoSurfaceDeleter {
    void operator()(cairo_surface_t* surface) const {
        cairo_surface_destroy(surface);
    }
};

using CairoSurfacePtr = std::unique_ptr<cairo_surface_t, SCairoSurfaceDeleter>;

CairoSurfacePtr rasterizeIcon(eBarIcon icon, const SIconColor& color, int surfaceSize);
