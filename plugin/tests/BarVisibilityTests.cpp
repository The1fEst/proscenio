#include "../BarVisibility.hpp"
#include "Expect.hpp"

#include <format>
#include <vector>

constexpr uint32_t MODE_UNSET  = 0;
constexpr uint32_t MODE_CLIENT = 1;
constexpr uint32_t MODE_SERVER = 2;

int main() {
    struct SCase {
        const char*         name;
        SBarVisibilityInput input;
        bool                expected;
    };

    const std::vector<SCase> CASES = {
        {"no decoration object draws its own frame", {}, false},
        {"xdg server side, as Dolphin asks", {.xdgDecorationMode = MODE_SERVER}, true},
        {"xdg client side", {.xdgDecorationMode = MODE_CLIENT}, false},
        {"xdg unset leaves the compositor to choose", {.xdgDecorationMode = MODE_UNSET}, true},
        {"kde server, as GTK without a header bar asks", {.kdeDecorationMode = MODE_SERVER}, true},
        {"kde client, as GTK3 with a header bar asks", {.kdeDecorationMode = MODE_CLIENT}, false},
        {"kde none", {.kdeDecorationMode = MODE_UNSET}, false},
        {"xdg client side with kde server", {.xdgDecorationMode = MODE_CLIENT, .kdeDecorationMode = MODE_SERVER}, true},
        {"xdg server side with kde client", {.xdgDecorationMode = MODE_SERVER, .kdeDecorationMode = MODE_CLIENT}, true},
        {"force_bar on a window that asked for nothing", {.hasForceBarRule = true}, true},
        {"force_bar on a window that draws its own frame", {.hasForceBarRule = true, .xdgDecorationMode = MODE_CLIENT}, true},
        {"no_bar on a window that asked for a bar", {.hasNoBarRule = true, .xdgDecorationMode = MODE_SERVER}, false},
        {"no_bar wins over force_bar", {.hasNoBarRule = true, .hasForceBarRule = true}, false},
        {"X11 asks for nothing and keeps its bar", {.isX11 = true}, true},
        {"no_bar on X11", {.isX11 = true, .hasNoBarRule = true}, false},
        {"override redirect X11", {.isOverrideRedirect = true, .isX11 = true}, false},
        {"force_bar on override redirect X11", {.isOverrideRedirect = true, .isX11 = true, .hasForceBarRule = true}, false},
        {"tiled window that asked for a bar", {.isFloating = false, .xdgDecorationMode = MODE_SERVER}, false},
        {"force_bar on a tiled window", {.isFloating = false, .hasForceBarRule = true}, false},
        {"tiled X11", {.isFloating = false, .isX11 = true}, false},
        {"tiled window that asked for a bar, all windows", {.isFloating = false, .isTiledAllowed = true, .xdgDecorationMode = MODE_SERVER}, true},
        {"tiled window that draws its own frame, all windows", {.isFloating = false, .isTiledAllowed = true, .xdgDecorationMode = MODE_CLIENT}, false},
        {"force_bar on a tiled window, all windows", {.isFloating = false, .isTiledAllowed = true, .hasForceBarRule = true}, true},
        {"no_bar on a tiled window, all windows", {.isFloating = false, .isTiledAllowed = true, .hasNoBarRule = true, .xdgDecorationMode = MODE_SERVER}, false},
        {"tiled X11, all windows", {.isFloating = false, .isTiledAllowed = true, .isX11 = true}, true},
        {"tiled override redirect X11, all windows", {.isFloating = false, .isTiledAllowed = true, .isOverrideRedirect = true, .isX11 = true}, false},
        {"floating window that asked for a bar, all windows", {.isTiledAllowed = true, .xdgDecorationMode = MODE_SERVER}, true},
        {"decorate off on a window that asked for a bar", {.isDecorated = false, .xdgDecorationMode = MODE_SERVER}, false},
        {"force_bar with decorate off", {.isDecorated = false, .hasForceBarRule = true}, false},
    };

    for (const auto& testCase : CASES)
        expect(shouldShowBar(testCase.input) == testCase.expected, std::format("bar visibility: {}", testCase.name));

    expect(formatWindowAddresses({}) == "[]", "window addresses: none");
    expect(formatWindowAddresses({0x55aa01}) == R"(["0x55aa01"])", "window addresses: one");
    expect(formatWindowAddresses({0x1a, 0xff}) == R"(["0x1a","0xff"])", "window addresses: two");

    return reportResult("BarVisibilityTests");
}
