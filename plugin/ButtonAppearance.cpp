#include "ButtonAppearance.hpp"

SButtonAppearance computeButtonAppearance(eBarButton button, const SButtonState& state, bool isWindowFocused) {
    SButtonAppearance appearance = {
        .usesRoleColor   = isWindowFocused || state.isOverGroup,
        .iconAlpha       = state.isOverGroup ? 1.F : 0.F,
        .stateLayerAlpha = 0,
    };

    if (state.pressedButton == button)
        appearance.stateLayerAlpha = PRESS_STATE_LAYER_ALPHA;
    else if (state.hoveredButton == button)
        appearance.stateLayerAlpha = HOVER_STATE_LAYER_ALPHA;

    return appearance;
}

const char* selectGlyphName(eBarButton button, bool isWindowMaximized) {
    switch (button) {
        case BAR_BUTTON_CLOSE: return "close";
        case BAR_BUTTON_MINIMIZE: return "remove";
        case BAR_BUTTON_MAXIMIZE: return isWindowMaximized ? "close_fullscreen" : "open_in_full";
    }

    return "";
}
