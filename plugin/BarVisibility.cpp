#include "BarVisibility.hpp"

#include <format>
#include <iterator>
#include <sstream>

eBuiltInBarRule matchBuiltInBarRule(const std::string& windowClass, const std::string& title) {
    if ((windowClass.empty() && title.empty()) || windowClass == "dev.fEst.Proscenio" || windowClass == "steam")
        return BUILT_IN_BAR_RULE_NO_BAR;

    if (windowClass.starts_with("hyprland-"))
        return BUILT_IN_BAR_RULE_FORCE_BAR;

    return BUILT_IN_BAR_RULE_NONE;
}

std::optional<eBarBehavior> parseBarBehavior(std::string_view value) {
    if (value == "float")
        return BAR_BEHAVIOR_FLOAT;

    if (value == "all")
        return BAR_BEHAVIOR_ALL;

    return std::nullopt;
}

bool shouldShowBar(const SBarVisibilityInput& input) {
    const bool IS_PLACEMENT_ALLOWED = input.isFloating || input.isTiledAllowed;

    if (!input.areBarsEnabled || !IS_PLACEMENT_ALLOWED || !input.isDecorated || input.isPinned || input.isOverrideRedirect || input.hasNoBarRule)
        return false;

    if (input.hasForceBarRule)
        return true;

    if (input.builtInRule == BUILT_IN_BAR_RULE_NO_BAR)
        return false;

    if (input.isX11 || input.builtInRule == BUILT_IN_BAR_RULE_FORCE_BAR)
        return true;

    if (input.isGtkClient && input.isFloating && !input.isTiledAllowed)
        return true;

    const bool IS_REQUESTED_BY_XDG = input.xdgDecorationMode && *input.xdgDecorationMode != XDG_DECORATION_MODE_CLIENT_SIDE;
    const bool IS_REQUESTED_BY_KDE = input.kdeDecorationMode == KDE_DECORATION_MODE_SERVER;

    return IS_REQUESTED_BY_XDG || IS_REQUESTED_BY_KDE;
}

bool hasGtkLibrary(std::istream& maps) {
    std::string line;
    while (std::getline(maps, line)) {
        const auto SLASH = line.rfind('/');
        if (SLASH == std::string::npos)
            continue;

        const auto FILE_NAME = std::string_view{line}.substr(SLASH + 1);
        if (FILE_NAME.starts_with("libgtk-3.so") || FILE_NAME.starts_with("libgtk-4.so"))
            return true;
    }

    return false;
}

bool isBarsRequest(std::string_view command, std::string_view commandName) {
    std::istringstream             stream{std::string{command}};
    const std::vector<std::string> WORDS{std::istream_iterator<std::string>{stream}, std::istream_iterator<std::string>{}};

    return WORDS.size() == 2 && WORDS[0] == commandName && WORDS[1] == "bars";
}

std::string formatWindowAddresses(const std::vector<uintptr_t>& addresses) {
    std::string reply = "[";

    for (const auto address : addresses)
        reply += std::format("{}\"0x{:x}\"", reply.size() > 1 ? "," : "", address);

    return reply + "]";
}
