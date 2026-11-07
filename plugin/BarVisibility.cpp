#include "BarVisibility.hpp"

#include <format>

bool shouldShowBar(const SBarVisibilityInput& input) {
    if (!input.isFloating || !input.isDecorated || input.isOverrideRedirect || input.hasNoBarRule)
        return false;

    if (input.isX11 || input.hasForceBarRule)
        return true;

    const bool IS_REQUESTED_BY_XDG = input.xdgDecorationMode && *input.xdgDecorationMode != XDG_DECORATION_MODE_CLIENT_SIDE;
    const bool IS_REQUESTED_BY_KDE = input.kdeDecorationMode == KDE_DECORATION_MODE_SERVER;

    return IS_REQUESTED_BY_XDG || IS_REQUESTED_BY_KDE;
}

std::string formatWindowAddresses(const std::vector<uintptr_t>& addresses) {
    std::string reply = "[";

    for (const auto address : addresses)
        reply += std::format("{}\"0x{:x}\"", reply.size() > 1 ? "," : "", address);

    return reply + "]";
}
