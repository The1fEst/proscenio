#include "ButtonLayout.hpp"

#include <algorithm>

constexpr double BUTTON_GAP  = 8.0;
constexpr double BAR_PADDING = 16.0;

bool             SLayoutBox::containsPoint(double pointX, double pointY) const {
    return pointX >= x && pointX < x + w && pointY >= y && pointY < y + h;
}

std::optional<eBarButton> SBarLayout::findButtonAt(double pointX, double pointY) const {
    for (size_t i = 0; i < buttons.size(); ++i) {
        if (buttons[i].hitBox.containsPoint(pointX, pointY))
            return static_cast<eBarButton>(i);
    }

    return std::nullopt;
}

SBarLayout computeBarLayout(double barWidth, double barHeight, eButtonSide side, double discSize) {
    constexpr std::array FROM_RIGHT_EDGE = {BAR_BUTTON_CLOSE, BAR_BUTTON_MAXIMIZE, BAR_BUTTON_MINIMIZE};
    constexpr std::array FROM_LEFT_EDGE  = {BAR_BUTTON_CLOSE, BAR_BUTTON_MINIMIZE, BAR_BUTTON_MAXIMIZE};

    const bool           IS_LEFT   = side == BUTTON_SIDE_LEFT;
    const double         DISC_SIZE = std::min(discSize, barHeight);

    SBarLayout           layout;
    double               hitStart  = 0;
    double               discStart = BAR_PADDING;

    for (const auto BUTTON : IS_LEFT ? FROM_LEFT_EDGE : FROM_RIGHT_EDGE) {
        const double HIT_END = discStart + DISC_SIZE + BUTTON_GAP / 2.0;
        if (HIT_END > barWidth)
            break;

        layout.buttons[BUTTON] = {
            .discBox = {IS_LEFT ? discStart : barWidth - discStart - DISC_SIZE, (barHeight - DISC_SIZE) / 2.0, DISC_SIZE, DISC_SIZE},
            .hitBox  = {IS_LEFT ? hitStart : barWidth - HIT_END, 0, HIT_END - hitStart, barHeight},
        };

        hitStart = HIT_END;
        discStart += DISC_SIZE + BUTTON_GAP;
    }

    layout.groupBox = {IS_LEFT ? 0 : barWidth - hitStart, 0, hitStart, barHeight};

    return layout;
}
