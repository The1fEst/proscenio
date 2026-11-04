#pragma once

#include <array>
#include <cstddef>
#include <cstdint>
#include <optional>

enum eBarButton : uint8_t {
    BAR_BUTTON_CLOSE = 0,
    BAR_BUTTON_MINIMIZE,
    BAR_BUTTON_MAXIMIZE,
};

constexpr size_t BAR_BUTTON_COUNT = 3;

enum eButtonSide : uint8_t {
    BUTTON_SIDE_RIGHT = 0,
    BUTTON_SIDE_LEFT,
};

struct SLayoutBox {
    double x = 0;
    double y = 0;
    double w = 0;
    double h = 0;

    bool   containsPoint(double pointX, double pointY) const;
    bool   operator==(const SLayoutBox&) const = default;
};

struct SButtonGeometry {
    SLayoutBox discBox;
    SLayoutBox hitBox;

    bool       operator==(const SButtonGeometry&) const = default;
};

struct SBarLayout {
    std::array<SButtonGeometry, BAR_BUTTON_COUNT> buttons;
    SLayoutBox                                    groupBox;

    std::optional<eBarButton>                     findButtonAt(double pointX, double pointY) const;
    bool                                          operator==(const SBarLayout&) const = default;
};

SBarLayout computeBarLayout(double barWidth, double barHeight, eButtonSide side, double discSize);
