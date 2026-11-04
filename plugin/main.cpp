#define WLR_USE_UNSTABLE

#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/desktop/view/window/Window.hpp>
#include <hyprland/src/desktop/view/window/WindowPresentation.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#include <hyprland/src/config/ConfigManager.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/desktop/rule/windowRule/WindowRuleEffectContainer.hpp>
#include <hyprland/src/state/MonitorState.hpp>

#include <algorithm>

#include "TitleBarDecoration.hpp"
#include "globals.hpp"

APICALL EXPORT std::string PLUGIN_API_VERSION() {
    return HYPRLAND_API_VERSION;
}

static void onNewWindow(PHLWINDOW window) {
    if (std::ranges::any_of(window->presentation().decorations(), [](const auto& decoration) { return decoration->getDisplayName() == BAR_DISPLAY_NAME; }))
        return;

    auto bar = Hyprutils::Memory::makeShared<CTitleBarDecoration>(window);
    g_pGlobalState->bars.emplace_back(bar);
    bar->m_self = bar;
    HyprlandAPI::addWindowDecoration(PHANDLE, window, std::move(bar));
}

static void onConfigReloaded() {
    for (const auto& bar : g_pGlobalState->bars) {
        if (bar)
            bar->onConfigReloaded();
    }
}

static void onUpdateWindowRules(PHLWINDOW window) {
    const auto IT = std::ranges::find_if(g_pGlobalState->bars, [&window](const auto& bar) { return bar && bar->window() == window; });
    if (IT == g_pGlobalState->bars.end())
        return;

    (*IT)->updateRules();
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    PHANDLE = handle;

    const std::string HASH        = __hyprland_api_get_hash();
    const std::string CLIENT_HASH = __hyprland_api_get_client_hash();

    if (HASH != CLIENT_HASH) {
        HyprlandAPI::addNotification(PHANDLE, "[proscenio_bars] Version mismatch: the headers do not match the running Hyprland", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[proscenio_bars] Version mismatch");
    }

    g_pGlobalState               = makeUnique<SGlobalState>();
    g_pGlobalState->noBarRuleIdx = Desktop::Rule::windowEffects()->registerEffect("proscenio_bars:no_bar");

    static auto WINDOW_OPEN_LISTENER   = Event::bus()->m_events.window.open.listen([](PHLWINDOW window) { onNewWindow(window); });
    static auto WINDOW_RULES_LISTENER  = Event::bus()->m_events.window.updateRules.listen([](PHLWINDOW window) { onUpdateWindowRules(window); });
    static auto CONFIG_RELOAD_LISTENER = Event::bus()->m_events.config.reloaded.listen([] { onConfigReloaded(); });

    auto&       config = g_pGlobalState->config;

    config.barHeight     = makeShared<Config::Values::CIntValue>("plugin:proscenio_bars:bar_height", "Height of the bar", 40, Config::Values::SIntValueOptions{.min = 1});
    config.barColor      = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:bar_color", "Color of the bar", 0xFF211F26);
    config.titleColor    = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:title_color", "Color of the title", 0xFFE6E0E9);
    config.titleFont     = makeShared<Config::Values::CStringValue>("plugin:proscenio_bars:title_font", "Font of the title", "Sans");
    config.titleSize     = makeShared<Config::Values::CIntValue>("plugin:proscenio_bars:title_size", "Size of the title", 14, Config::Values::SIntValueOptions{.min = 1});
    config.titleWeight   = makeShared<Config::Values::CFontWeightValue>("plugin:proscenio_bars:title_weight", "Weight of the title (e.g. \"bold\" or an integer 100-1000)", 500);
    config.buttonSide    = makeShared<Config::Values::CStringValue>("plugin:proscenio_bars:button_side", "Side of the bar the buttons sit on: right or left", "right");
    config.closeColor    = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:close_color", "Color of the close button", 0xFFF2B8B5);
    config.minimizeColor = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:minimize_color", "Color of the minimize button", 0xFFEFB8C8);
    config.maximizeColor = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:maximize_color", "Color of the maximize button", 0xFFD0BCFF);

    HyprlandAPI::addConfigValueV2(PHANDLE, config.barHeight);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.barColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleFont);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleSize);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleWeight);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.buttonSide);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.closeColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.minimizeColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.maximizeColor);

    for (const auto& window : Desktop::windowState()->windows()) {
        if (validMapped(window))
            onNewWindow(window);
    }

    HyprlandAPI::reloadConfig();

    return {"proscenio_bars", "Title bars for floating windows.", "fEst", "1.0"};
}

APICALL EXPORT void PLUGIN_EXIT() {
    for (const auto& monitor : State::monitorState()->monitors())
        monitor->m_scheduledRecalc = true;

    g_pHyprRenderer->context().m_pass.removeAllOfType("CTitleBarPassElement");

    Desktop::Rule::windowEffects()->unregisterEffect(g_pGlobalState->noBarRuleIdx);
}
