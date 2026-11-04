#include "../IconRasterizer.hpp"
#include "Expect.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <format>
#include <vector>

struct SInkBounds {
    int minX = 1 << 20;
    int minY = 1 << 20;
    int maxX = -1;
    int maxY = -1;
};

struct SDesignPoint {
    double x = 0;
    double y = 0;
};

struct SIconShape {
    const char*               name;
    eBarIcon                  icon;
    std::vector<SDesignPoint> inkedPoints;
    std::vector<SDesignPoint> emptyPoints;
    double                    inkArea;
    bool                      isVerticallySymmetric;
};

constexpr double                     DESIGN_SIZE        = 16;
constexpr double                     INKED_COVERAGE     = 0.7;
constexpr double                     EMPTY_COVERAGE     = 0.02;
constexpr double                     INK_AREA_TOLERANCE = 0.03;
constexpr int                        MIRROR_TOLERANCE   = 10;
constexpr uint8_t                    INK_ALPHA_FLOOR    = 64;

static const std::vector<int>        SIZES = {16, 26, 40};

static const std::vector<SIconShape> SHAPES = {
    {
        "minimize",
        BAR_ICON_MINIMIZE,
        {{-4, -2}, {-2, 0}, {0, 2}, {2, 0}, {4, -2}},
        {{0, -5}, {0, -4}, {0, -2}, {0, 5}, {-4, 2}, {4, 2}},
        25.56,
        false,
    },
    {
        "maximize",
        BAR_ICON_MAXIMIZE,
        {{-4, 2}, {-2, 0}, {0, -2}, {2, 0}, {4, 2}},
        {{0, 5}, {0, 4}, {0, 2}, {0, -5}, {-4, -2}, {4, -2}},
        25.56,
        false,
    },
    {
        "restore",
        BAR_ICON_RESTORE,
        {{-4, 0}, {-2, -2}, {0, -4}, {2, -2}, {4, 0}, {2, 2}, {0, 4}, {-2, 2}},
        {{0, 0}, {-5, -5}, {5, -5}, {-5, 5}, {5, 5}},
        44.39,
        true,
    },
    {
        "close",
        BAR_ICON_CLOSE,
        {{-4, -4}, {-2, -2}, {0, 0}, {2, 2}, {4, 4}, {4, -4}, {2, -2}, {-2, 2}, {-4, 4}},
        {{0, -4}, {0, 4}, {-4, 0}, {4, 0}},
        47.54,
        true,
    },
};

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

static double sampleCoverage(cairo_surface_t* surface, SDesignPoint point) {
    const int    SIZE            = cairo_image_surface_get_width(surface);
    const double PIXELS_PER_UNIT = SIZE / DESIGN_SIZE;
    const double X               = SIZE / 2.0 + point.x * PIXELS_PER_UNIT - 0.5;
    const double Y               = SIZE / 2.0 + point.y * PIXELS_PER_UNIT - 0.5;
    const int    COLUMN          = std::floor(X);
    const int    ROW             = std::floor(Y);
    const double WEIGHT_X        = X - COLUMN;
    const double WEIGHT_Y        = Y - ROW;
    const auto   readCoverage    = [&](int x, int y) { return findPixel(surface, x, y)[3] / 255.0; };

    return (readCoverage(COLUMN, ROW) * (1 - WEIGHT_X) + readCoverage(COLUMN + 1, ROW) * WEIGHT_X) * (1 - WEIGHT_Y) +
        (readCoverage(COLUMN, ROW + 1) * (1 - WEIGHT_X) + readCoverage(COLUMN + 1, ROW + 1) * WEIGHT_X) * WEIGHT_Y;
}

static double measureInkArea(cairo_surface_t* surface) {
    const int SIZE = cairo_image_surface_get_width(surface);
    double    area = 0;

    for (int y = 0; y < SIZE; ++y) {
        for (int x = 0; x < SIZE; ++x)
            area += findPixel(surface, x, y)[3] / 255.0;
    }

    return area;
}

static void testIconsDrawTheirShapes() {
    for (const auto& shape : SHAPES) {
        for (const int size : SIZES) {
            const auto SURFACE = rasterizeIcon(shape.icon, {1, 1, 1, 1}, size);
            const auto LABEL   = std::format("{} at {}", shape.name, size);

            for (const auto& point : shape.inkedPoints)
                expect(sampleCoverage(SURFACE.get(), point) >= INKED_COVERAGE, std::format("inked at ({}, {}): {}", point.x, point.y, LABEL));

            for (const auto& point : shape.emptyPoints)
                expect(sampleCoverage(SURFACE.get(), point) <= EMPTY_COVERAGE, std::format("empty at ({}, {}): {}", point.x, point.y, LABEL));

            const double DESIGN_AREA = measureInkArea(SURFACE.get()) * std::pow(DESIGN_SIZE / size, 2);
            expect(std::abs(DESIGN_AREA - shape.inkArea) <= shape.inkArea * INK_AREA_TOLERANCE, std::format("ink area {:.2f} matches the stroke: {}", DESIGN_AREA, LABEL));
        }
    }
}

static void testStrokeCapsAndJoinsAreRound() {
    const auto SURFACE = rasterizeIcon(BAR_ICON_MINIMIZE, {1, 1, 1, 1}, 160);

    expect(sampleCoverage(SURFACE.get(), {-4.64, -2.64}) >= INKED_COVERAGE, "the cap reaches past the end of the line");
    expect(sampleCoverage(SURFACE.get(), {-4, -3.27}) <= EMPTY_COVERAGE, "the cap has no square corner");
    expect(sampleCoverage(SURFACE.get(), {0, 2.9}) >= INKED_COVERAGE, "the join is not beveled");
    expect(sampleCoverage(SURFACE.get(), {0, 3.3}) <= EMPTY_COVERAGE, "the join is not mitered");
}

static void testIconsAreSymmetric() {
    for (const auto& shape : SHAPES) {
        for (const int size : SIZES) {
            const auto SURFACE              = rasterizeIcon(shape.icon, {1, 1, 1, 1}, size);
            int        horizontalDifference = 0;
            int        verticalDifference   = 0;

            for (int y = 0; y < size; ++y) {
                for (int x = 0; x < size; ++x) {
                    const int ALPHA      = findPixel(SURFACE.get(), x, y)[3];
                    horizontalDifference = std::max(horizontalDifference, std::abs(ALPHA - findPixel(SURFACE.get(), size - 1 - x, y)[3]));
                    verticalDifference   = std::max(verticalDifference, std::abs(ALPHA - findPixel(SURFACE.get(), x, size - 1 - y)[3]));
                }
            }

            expect(horizontalDifference <= MIRROR_TOLERANCE, std::format("left mirrors right, off by {}: {} at {}", horizontalDifference, shape.name, size));

            if (shape.isVerticallySymmetric)
                expect(verticalDifference <= MIRROR_TOLERANCE, std::format("top mirrors bottom, off by {}: {} at {}", verticalDifference, shape.name, size));
        }
    }
}

static void testIconsAreCenteredAndFit() {
    for (const auto& shape : SHAPES) {
        for (const int size : SIZES) {
            const auto SURFACE = rasterizeIcon(shape.icon, {1, 1, 1, 1}, size);
            const auto LABEL   = std::format("{} at {}", shape.name, size);

            expect(cairo_image_surface_get_width(SURFACE.get()) == size && cairo_image_surface_get_height(SURFACE.get()) == size, "surface is the disc size: " + LABEL);
            expect(cairo_image_surface_get_format(SURFACE.get()) == CAIRO_FORMAT_ARGB32, "surface is ARGB32: " + LABEL);

            const auto INK = measureInk(SURFACE.get());
            expect(INK.maxX >= 0, "icon has ink: " + LABEL);
            expect(INK.minX > 0 && INK.minY > 0 && INK.maxX < size - 1 && INK.maxY < size - 1, "ink fits inside the surface: " + LABEL);
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
        const auto SURFACE = rasterizeIcon(BAR_ICON_CLOSE, testCase.color, 64);
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

    const auto SURFACE         = rasterizeIcon(BAR_ICON_CLOSE, {1, 1, 1, 1}, 64);
    bool       isPremultiplied = true;

    for (int y = 0; y < 64; ++y) {
        for (int x = 0; x < 64; ++x) {
            const auto* PIXEL = findPixel(SURFACE.get(), x, y);
            isPremultiplied   = isPremultiplied && PIXEL[0] <= PIXEL[3] && PIXEL[1] <= PIXEL[3] && PIXEL[2] <= PIXEL[3];
        }
    }

    expect(isPremultiplied, "no channel exceeds alpha");
}

static void testTranslucentCrossingIsBlendedOnce() {
    const auto SURFACE = rasterizeIcon(BAR_ICON_CLOSE, {1, 1, 1, 0.5}, 64);
    expect(std::abs(findPixel(SURFACE.get(), 32, 32)[3] - 128) <= 1, "the crossing of the close lines keeps the color's alpha");
}

int main() {
    testIconsDrawTheirShapes();
    testStrokeCapsAndJoinsAreRound();
    testIconsAreSymmetric();
    testIconsAreCenteredAndFit();
    testPixelsArePremultipliedBgra();
    testTranslucentCrossingIsBlendedOnce();

    return reportResult("IconRasterizerTests");
}
