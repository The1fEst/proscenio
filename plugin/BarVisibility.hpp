#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

constexpr uint32_t XDG_DECORATION_MODE_CLIENT_SIDE = 1;
constexpr uint32_t KDE_DECORATION_MODE_SERVER      = 2;

struct SBarVisibilityInput {
    bool                    isFloating         = true;
    bool                    isTiledAllowed     = false;
    bool                    isDecorated        = true;
    bool                    isOverrideRedirect = false;
    bool                    isX11              = false;
    bool                    hasNoBarRule       = false;
    bool                    hasForceBarRule    = false;
    std::optional<uint32_t> xdgDecorationMode;
    std::optional<uint32_t> kdeDecorationMode;
};

bool        shouldShowBar(const SBarVisibilityInput& input);
std::string formatWindowAddresses(const std::vector<uintptr_t>& addresses);
