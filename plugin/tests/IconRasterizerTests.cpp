#include "../IconRasterizer.hpp"
#include "Expect.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <format>
#include <vector>

struct SInkBounds {
    int minX = 1 << 20;
    int minY = 1 << 20;
    int maxX = -1;
    int maxY = -1;
};

constexpr uint8_t     INK_ALPHA_FLOOR = 64;

static const uint8_t* findPixel(cairo_surface_t* surface, int x, int y) {
    return cairo_image_surface_get_data(surface) + y * cairo_image_surface_get_stride(surface) + x * 4;
}

static SInkBounds measureInk(cairo_surface_t* surface) {
    const int  SIZE = cairo_image_surface_get_width(surface);
    SInkBounds bounds;

    for (int y = 0; y < SIZE; ++y) {
        for (int x = 0; x < SIZE; ++x) {
            if (findPixel(surface, x, y)[3] < INK_ALPHA_FLOOR)
                continue;

            bounds.minX = std::min(bounds.minX, x);
            bounds.minY = std::min(bounds.minY, y);
            bounds.maxX = std::max(bounds.maxX, x);
            bounds.maxY = std::max(bounds.maxY, y);
        }
    }

    return bounds;
}

static void testGlyphsAreCenteredAndFit() {
    const std::vector<const char*> GLYPHS = {"close", "remove", "open_in_full", "close_fullscreen"};
    const std::vector<int>         SIZES  = {16, 20, 24, 32, 40};

    for (const auto* glyph : GLYPHS) {
        for (const int size : SIZES) {
            const auto SURFACE = rasterizeIcon(glyph, {1, 1, 1, 1}, size);
            const auto LABEL   = std::format("{} at {}", glyph, size);

            expect(cairo_image_surface_get_width(SURFACE.get()) == size && cairo_image_surface_get_height(SURFACE.get()) == size, "surface is the disc size: " + LABEL);
            expect(cairo_image_surface_get_format(SURFACE.get()) == CAIRO_FORMAT_ARGB32, "surface is ARGB32: " + LABEL);

            const auto INK = measureInk(SURFACE.get());
            expect(INK.maxX >= 0, "glyph has ink: " + LABEL);
            expect(INK.minX > 0 && INK.minY > 0 && INK.maxX < size - 1 && INK.maxY < size - 1, "ink fits inside the surface (is the font installed?): " + LABEL);
            expect(std::abs((INK.minX + INK.maxX + 1) / 2.0 - size / 2.0) <= 1.0, "ink is centered horizontally: " + LABEL);
            expect(std::abs((INK.minY + INK.maxY + 1) / 2.0 - size / 2.0) <= 1.0, "ink is centered vertically: " + LABEL);
        }
    }
}

static void testPixelsArePremultipliedBgra() {
    struct SCase {
        const char* name;
        SIconColor  color;
        uint8_t     expectedBlue;
        uint8_t     expectedGreen;
        uint8_t     expectedRed;
    };

    const std::vector<SCase> CASES = {
        {"red", {1, 0, 0, 1}, 0, 0, 255},
        {"green", {0, 1, 0, 1}, 0, 255, 0},
        {"blue", {0, 0, 1, 1}, 255, 0, 0},
        {"on_error", {0x60 / 255.0, 0x14 / 255.0, 0x10 / 255.0, 1}, 0x10, 0x14, 0x60},
    };

    for (const auto& testCase : CASES) {
        const auto SURFACE = rasterizeIcon("close", testCase.color, 64);
        int        peakX   = 0;
        int        peakY   = 0;

        for (int y = 0; y < 64; ++y) {
            for (int x = 0; x < 64; ++x) {
                if (findPixel(SURFACE.get(), x, y)[3] > findPixel(SURFACE.get(), peakX, peakY)[3]) {
                    peakX = x;
                    peakY = y;
                }
            }
        }

        const auto* PIXEL = findPixel(SURFACE.get(), peakX, peakY);
        expect(PIXEL[3] == 255, std::format("{}: a pixel is fully opaque", testCase.name));
        expect(PIXEL[0] == testCase.expectedBlue, std::format("{}: byte 0 is blue", testCase.name));
        expect(PIXEL[1] == testCase.expectedGreen, std::format("{}: byte 1 is green", testCase.name));
        expect(PIXEL[2] == testCase.expectedRed, std::format("{}: byte 2 is red", testCase.name));
    }

    const auto SURFACE         = rasterizeIcon("close", {1, 1, 1, 1}, 64);
    bool       isPremultiplied = true;

    for (int y = 0; y < 64; ++y) {
        for (int x = 0; x < 64; ++x) {
            const auto* PIXEL = findPixel(SURFACE.get(), x, y);
            isPremultiplied   = isPremultiplied && PIXEL[0] <= PIXEL[3] && PIXEL[1] <= PIXEL[3] && PIXEL[2] <= PIXEL[3];
        }
    }

    expect(isPremultiplied, "no channel exceeds alpha");
}

static void testTextWiderThanTheSurfaceDrawsNothing() {
    const auto SURFACE = rasterizeIcon("not a glyph name, so it shapes as plain text", {1, 1, 1, 1}, 16);
    expect(measureInk(SURFACE.get()).maxX < 0, "plain text leaves the surface empty");
}

int main() {
    testTextWiderThanTheSurfaceDrawsNothing();
    testGlyphsAreCenteredAndFit();
    testPixelsArePremultipliedBgra();

    return reportResult("IconRasterizerTests");
}
