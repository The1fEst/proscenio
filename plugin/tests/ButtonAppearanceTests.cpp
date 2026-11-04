#include "../ButtonAppearance.hpp"
#include "Expect.hpp"

#include <format>
#include <string_view>
#include <vector>

static void testAppearance() {
    struct SCase {
        const char*       name;
        eBarButton        button;
        SButtonState      state;
        bool              isWindowFocused;
        SButtonAppearance expected;
    };

    constexpr SButtonState   IDLE        = {};
    constexpr SButtonState   GROUP_ONLY  = {.isOverGroup = true};
    constexpr SButtonState   CLOSE_HOVER = {.hoveredButton = BAR_BUTTON_CLOSE, .isOverGroup = true};
    constexpr SButtonState   CLOSE_PRESS = {.hoveredButton = BAR_BUTTON_CLOSE, .pressedButton = BAR_BUTTON_CLOSE, .isOverGroup = true};

    const std::vector<SCase> CASES = {
        {"focused and idle: role color, no icon", BAR_BUTTON_MINIMIZE, IDLE, true, {.usesRoleColor = true}},
        {"unfocused and idle: inactive, no icon", BAR_BUTTON_MINIMIZE, IDLE, false, {.usesRoleColor = false}},
        {"focused over the group: icon", BAR_BUTTON_MAXIMIZE, GROUP_ONLY, true, {.usesRoleColor = true, .iconAlpha = 1}},
        {"unfocused over the group: role color and icon", BAR_BUTTON_CLOSE, GROUP_ONLY, false, {.usesRoleColor = true, .iconAlpha = 1}},
        {"hovered button: hover layer", BAR_BUTTON_CLOSE, CLOSE_HOVER, true, {.usesRoleColor = true, .iconAlpha = 1, .stateLayerAlpha = HOVER_STATE_LAYER_ALPHA}},
        {"hovered button unfocused: hover layer", BAR_BUTTON_CLOSE, CLOSE_HOVER, false, {.usesRoleColor = true, .iconAlpha = 1, .stateLayerAlpha = HOVER_STATE_LAYER_ALPHA}},
        {"neighbor of a hovered button: icon only", BAR_BUTTON_MINIMIZE, CLOSE_HOVER, true, {.usesRoleColor = true, .iconAlpha = 1}},
        {"pressed button: press layer", BAR_BUTTON_CLOSE, CLOSE_PRESS, true, {.usesRoleColor = true, .iconAlpha = 1, .stateLayerAlpha = PRESS_STATE_LAYER_ALPHA}},
        {"neighbor of a pressed button: icon only", BAR_BUTTON_MAXIMIZE, CLOSE_PRESS, true, {.usesRoleColor = true, .iconAlpha = 1}},
    };

    for (const auto& testCase : CASES)
        expect(computeButtonAppearance(testCase.button, testCase.state, testCase.isWindowFocused) == testCase.expected, std::format("appearance: {}", testCase.name));
}

static void testArmedButton() {
    const auto          LAYOUT = computeBarLayout(300, 40, BUTTON_SIDE_RIGHT, 16);
    CButtonStateMachine machine;

    const auto          computeCloseAppearance = [&machine] { return computeButtonAppearance(BAR_BUTTON_CLOSE, machine.state(), true); };

    machine.onPointerMove(LAYOUT, 280, 20);
    machine.onPress();
    expect(computeCloseAppearance() == SButtonAppearance{.usesRoleColor = true, .iconAlpha = 1, .stateLayerAlpha = PRESS_STATE_LAYER_ALPHA}, "armed on the button: press layer");

    machine.onPointerMove(LAYOUT, 250, 20);
    expect(computeCloseAppearance() == SButtonAppearance{.usesRoleColor = true, .iconAlpha = 1}, "armed on a neighbor: icon and no layer");
    expect(computeButtonAppearance(BAR_BUTTON_MAXIMIZE, machine.state(), true) == SButtonAppearance{.usesRoleColor = true, .iconAlpha = 1},
           "neighbor of an armed button: no layer");

    machine.onPointerMove(LAYOUT, 100, 20);
    expect(computeCloseAppearance() == SButtonAppearance{.usesRoleColor = true}, "armed on the free area: no icon and no layer");

    machine.onPointerLeave();
    expect(computeCloseAppearance() == SButtonAppearance{.usesRoleColor = true}, "armed off the bar: no icon and no layer");

    machine.onPointerMove(LAYOUT, 280, 20);
    expect(computeCloseAppearance() == SButtonAppearance{.usesRoleColor = true, .iconAlpha = 1, .stateLayerAlpha = PRESS_STATE_LAYER_ALPHA}, "back on the button: press layer");
}

static void testGlyphNames() {
    struct SCase {
        eBarButton       button;
        bool             isWindowMaximized;
        std::string_view expected;
    };

    const std::vector<SCase> CASES = {
        {BAR_BUTTON_CLOSE, false, "close"},           {BAR_BUTTON_CLOSE, true, "close"},
        {BAR_BUTTON_MINIMIZE, false, "remove"},       {BAR_BUTTON_MINIMIZE, true, "remove"},
        {BAR_BUTTON_MAXIMIZE, false, "open_in_full"}, {BAR_BUTTON_MAXIMIZE, true, "close_fullscreen"},
    };

    for (const auto& testCase : CASES)
        expect(selectGlyphName(testCase.button, testCase.isWindowMaximized) == testCase.expected, std::format("glyph: {}", testCase.expected));
}

int main() {
    testAppearance();
    testArmedButton();
    testGlyphNames();

    return reportResult("ButtonAppearanceTests");
}
