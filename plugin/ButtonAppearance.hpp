#pragma once

#include "ButtonStateMachine.hpp"

constexpr float HOVER_STATE_LAYER_ALPHA = 0.08F;
constexpr float PRESS_STATE_LAYER_ALPHA = 0.10F;

struct SButtonAppearance {
    bool  usesRoleColor   = false;
    float iconAlpha       = 0;
    float stateLayerAlpha = 0;

    bool  operator==(const SButtonAppearance&) const = default;
};

SButtonAppearance computeButtonAppearance(eBarButton button, const SButtonState& state, bool isWindowFocused);
const char*       selectGlyphName(eBarButton button, bool isWindowMaximized);
