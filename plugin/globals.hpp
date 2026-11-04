#pragma once

#include <hyprland/src/plugins/PluginAPI.hpp>
#include <hyprland/src/config/values/types/IntValue.hpp>
#include <hyprland/src/config/values/types/StringValue.hpp>
#include <hyprland/src/config/values/types/ColorValue.hpp>
#include <hyprland/src/config/values/types/FontWeightValue.hpp>

#include <vector>

inline HANDLE         PHANDLE          = nullptr;
inline constexpr auto BAR_DISPLAY_NAME = "proscenio_bars";

class CTitleBarDecoration;

struct SGlobalState {
    std::vector<WP<CTitleBarDecoration>> bars;
    uint32_t                             noBarRuleIdx = 0;

    struct {
        SP<Config::Values::CIntValue>        barHeight, titleSize;
        SP<Config::Values::CFontWeightValue> titleWeight;
        SP<Config::Values::CStringValue>     titleFont, buttonSide;
        SP<Config::Values::CColorValue>      barColor, titleColor, closeColor, minimizeColor, maximizeColor;
    } config;
};

inline UP<SGlobalState> g_pGlobalState;
