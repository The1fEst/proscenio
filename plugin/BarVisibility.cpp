#include "BarVisibility.hpp"

#include <format>

eBuiltInBarRule matchBuiltInBarRule(const std::string& windowClass, const std::string& title) {
    if ((windowClass.empty() && title.empty()) || windowClass == "dev.fEst.Proscenio" || windowClass == "steam")
        return BUILT_IN_BAR_RULE_NO_BAR;

    if (windowClass.starts_with("hyprland-"))
        return BUILT_IN_BAR_RULE_FORCE_BAR;

    return BUILT_IN_BAR_RULE_NONE;
}

bool shouldShowBar(const SBarVisibilityInput& input) {
    const bool IS_PLACEMENT_ALLOWED = input.isFloating || input.isTiledAllowed;

    if (!IS_PLACEMENT_ALLOWED || !input.isDecorated || input.isOverrideRedirect || input.hasNoBarRule)
        return false;

    if (input.hasForceBarRule)
        return true;

    if (input.builtInRule == BUILT_IN_BAR_RULE_NO_BAR)
        return false;

    if (input.isX11 || input.builtInRule == BUILT_IN_BAR_RULE_FORCE_BAR)
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
