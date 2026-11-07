#include "BarVisibility.hpp"

bool shouldShowBar(const SBarVisibilityInput& input) {
    if (!input.isFloating || !input.isDecorated || input.isOverrideRedirect || input.hasNoBarRule)
        return false;

    if (input.isX11 || input.hasForceBarRule)
        return true;

    const bool IS_REQUESTED_BY_XDG = input.xdgDecorationMode && *input.xdgDecorationMode != XDG_DECORATION_MODE_CLIENT_SIDE;
    const bool IS_REQUESTED_BY_KDE = input.kdeDecorationMode == KDE_DECORATION_MODE_SERVER;

    return IS_REQUESTED_BY_XDG || IS_REQUESTED_BY_KDE;
}
