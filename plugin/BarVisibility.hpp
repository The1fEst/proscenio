#pragma once

#include <cstdint>
#include <istream>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

constexpr uint32_t XDG_DECORATION_MODE_CLIENT_SIDE = 1;
constexpr uint32_t KDE_DECORATION_MODE_SERVER      = 2;

enum eBuiltInBarRule : uint8_t {
    BUILT_IN_BAR_RULE_NONE,
    BUILT_IN_BAR_RULE_NO_BAR,
    BUILT_IN_BAR_RULE_FORCE_BAR,
};

enum eBarBehavior : uint8_t {
    BAR_BEHAVIOR_FLOAT,
    BAR_BEHAVIOR_ALL,
};

struct SBarVisibilityInput {
    bool                    areBarsEnabled     = true;
    bool                    isFloating         = true;
    bool                    isTiledAllowed     = false;
    bool                    isDecorated        = true;
    bool                    isPinned           = false;
    bool                    isOverrideRedirect = false;
    bool                    isX11              = false;
    bool                    hasNoBarRule       = false;
    bool                    hasForceBarRule    = false;
    bool                    isGtkClient        = false;
    eBuiltInBarRule         builtInRule        = BUILT_IN_BAR_RULE_NONE;
    std::optional<uint32_t> xdgDecorationMode;
    std::optional<uint32_t> kdeDecorationMode;
};

eBuiltInBarRule             matchBuiltInBarRule(const std::string& windowClass, const std::string& title);
std::optional<eBarBehavior> parseBarBehavior(std::string_view value);
bool                        shouldShowBar(const SBarVisibilityInput& input);
bool                        hasGtkLibrary(std::istream& maps);
bool                        isBarsRequest(std::string_view command, std::string_view commandName);
std::string                 formatWindowAddresses(const std::vector<uintptr_t>& addresses);
