#pragma once

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/config/values/types/IntValue.hpp>
#include <hyprland/src/config/values/types/StringValue.hpp>
#include <hyprland/src/config/values/types/ColorValue.hpp>
#include <hyprland/src/config/values/types/FontWeightValue.hpp>
#include <hyprland/src/render/Texture.hpp>
#include <hyprutils/animation/AnimationConfig.hpp>

#include "ButtonLayout.hpp"

#include <compare>
#include <map>
#include <vector>

inline HANDLE         PHANDLE          = nullptr;
inline constexpr auto BAR_DISPLAY_NAME = "proscenio_bars";

class CTitleBarDecoration;

namespace Config {
    class CWorkspaceRule;
}

struct SIconKey {
    eBarButton      button     = BAR_BUTTON_CLOSE;
    eBarIcon        icon       = BAR_ICON_CLOSE;
    Config::INTEGER color      = 0;
    int             deviceSize = 0;

    auto            operator<=>(const SIconKey&) const = default;
};

struct SGlobalState {
    std::vector<WP<CTitleBarDecoration>>               bars;
    std::map<SIconKey, SP<Render::ITexture>>           iconTextures;
    std::map<std::string, WP<Config::CWorkspaceRule>>  monocleRules;
    SP<Hyprutils::Animation::SAnimationPropertyConfig> animationConfig;
    uint32_t                                           noBarRuleIdx    = 0;
    uint32_t                                           forceBarRuleIdx = 0;

    struct {
        SP<Config::Values::CIntValue>        barHeight, titleSize;
        SP<Config::Values::CFontWeightValue> titleWeight;
        SP<Config::Values::CStringValue>     titleFont, buttonSide, windows;
        SP<Config::Values::CColorValue>      barColor, inactiveBarColor, titleColor, inactiveTitleColor;
        SP<Config::Values::CColorValue>      closeColor, closeIconColor, minimizeColor, minimizeIconColor, maximizeColor, maximizeIconColor, inactiveButtonColor;
    } config;
};

inline UP<SGlobalState> g_pGlobalState;
