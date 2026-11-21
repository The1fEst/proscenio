#define WLR_USE_UNSTABLE

#include <hyprland/src/Compositor.hpp>
#include <hyprland/src/desktop/view/window/Window.hpp>
#include <hyprland/src/desktop/view/window/WindowPresentation.hpp>
#include <hyprland/src/desktop/state/FocusState.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#include <hyprland/src/config/ConfigManager.hpp>
#include <hyprland/src/config/shared/animation/AnimationTree.hpp>
#include <hyprland/src/render/Renderer.hpp>
#include <hyprland/src/event/EventBus.hpp>
#include <hyprland/src/desktop/rule/windowRule/WindowRuleEffectContainer.hpp>
#include <hyprland/src/state/MonitorState.hpp>

#include <algorithm>
#include <linux/input-event-codes.h>

#include "BarVisibility.hpp"
#include "ScrollingToggle.hpp"
#include "TitleBarDecoration.hpp"
#include "globals.hpp"

constexpr float ANIMATION_SPEED = 1.5F;

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
    g_pGlobalState->iconTextures.clear();
    g_pGlobalState->animationConfig->internalEnabled = Config::animationTree()->getAnimationPropertyConfig("fade")->pValues->internalEnabled;

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

static void onMouseButton(IPointer::SButtonEvent event, Event::SCallbackInfo& info) {
    const auto BARS = g_pGlobalState->bars;

    if (event.state == WL_POINTER_BUTTON_STATE_PRESSED) {
        for (const auto& bar : BARS) {
            if (bar)
                bar->dropStalePress(event.button);
        }
    }

    for (const auto& bar : BARS) {
        if (bar)
            bar->onMouseButton(info, event);
    }

    for (const auto& bar : BARS) {
        if (bar)
            bar->updatePointer();
    }

    if (event.button == BTN_LEFT && event.state == WL_POINTER_BUTTON_STATE_RELEASED && !info.cancelled)
        centerClickedColumn(Desktop::focusState()->window());
}

static IPC::Socket1::SResponse listWindowsWithBar(const IPC::Socket1::SRequest&) {
    std::vector<uintptr_t> addresses;

    for (const auto& bar : g_pGlobalState->bars) {
        if (bar && bar->isBarVisible())
            addresses.emplace_back(rc<uintptr_t>(bar->window().get()));
    }

    return formatWindowAddresses(addresses);
}

static void onWindowActive(PHLWINDOW window, Desktop::eFocusReason reason) {
    for (const auto& bar : g_pGlobalState->bars) {
        if (!bar)
            continue;

        bar->updateFocus();
        bar->updatePointer();
    }

    if (validMapped(window) && reason != Desktop::FOCUS_REASON_FFM && !(reason & Desktop::FOCUS_REASON_CLICK))
        centerFocusedColumn(window->m_workspace);
}

static void onMonitorLayoutChanged() {
    for (const auto& monitor : State::monitorState()->monitors()) {
        resizeScrollingColumns(monitor->m_activeWorkspace);
        resizeScrollingColumns(monitor->m_activeSpecialWorkspace);
    }
}

APICALL EXPORT PLUGIN_DESCRIPTION_INFO PLUGIN_INIT(HANDLE handle) {
    PHANDLE = handle;

    const std::string HASH        = __hyprland_api_get_hash();
    const std::string CLIENT_HASH = __hyprland_api_get_client_hash();

    if (HASH != CLIENT_HASH) {
        HyprlandAPI::addNotification(PHANDLE, "[proscenio_bars] Version mismatch: the headers do not match the running Hyprland", CHyprColor{1.0, 0.2, 0.2, 1.0}, 5000);
        throw std::runtime_error("[proscenio_bars] Version mismatch");
    }

    g_pGlobalState                  = makeUnique<SGlobalState>();
    g_pGlobalState->noBarRuleIdx    = Desktop::Rule::windowEffects()->registerEffect("proscenio_bars:no_bar");
    g_pGlobalState->forceBarRuleIdx = Desktop::Rule::windowEffects()->registerEffect("proscenio_bars:force_bar");

    g_pGlobalState->animationConfig                  = makeShared<Hyprutils::Animation::SAnimationPropertyConfig>();
    g_pGlobalState->animationConfig->internalEnabled = 1;
    g_pGlobalState->animationConfig->internalSpeed   = ANIMATION_SPEED;
    g_pGlobalState->animationConfig->internalBezier  = "default";
    g_pGlobalState->animationConfig->pValues         = g_pGlobalState->animationConfig;

    static auto WINDOW_OPEN_LISTENER   = Event::bus()->m_events.window.open.listen([](PHLWINDOW window) { onNewWindow(window); });
    static auto WINDOW_RULES_LISTENER  = Event::bus()->m_events.window.updateRules.listen([](PHLWINDOW window) { onUpdateWindowRules(window); });
    static auto CONFIG_RELOAD_LISTENER = Event::bus()->m_events.config.reloaded.listen([] { onConfigReloaded(); });
    static auto MOUSE_BUTTON_LISTENER =
        Event::bus()->m_events.input.mouse.button.listen([](IPointer::SButtonEvent event, Event::SCallbackInfo& info) { onMouseButton(event, info); });
    static auto WINDOW_ACTIVE_LISTENER = Event::bus()->m_events.window.active.listen([](PHLWINDOW window, Desktop::eFocusReason reason) { onWindowActive(window, reason); });

    static auto SCROLLING_OPEN_LISTENER      = Event::bus()->m_events.window.open.listen([](PHLWINDOW window) { resizeScrollingColumns(window->m_workspace); });
    static auto SCROLLING_FLOATING_LISTENER  = Event::bus()->m_events.window.floating.listen([](PHLWINDOW window) {
        if (window)
            resizeScrollingColumns(window->m_workspace);
    });
    static auto SCROLLING_MONITOR_LISTENER   = Event::bus()->m_events.monitor.layoutChanged.listen([] { onMonitorLayoutChanged(); });
    static auto SCROLLING_WORKSPACE_LISTENER = Event::bus()->m_events.workspace.moveToMonitor.listen([](PHLWORKSPACE workspace, PHLMONITOR) { resizeScrollingColumns(workspace); });

    auto&       config = g_pGlobalState->config;

    config.barHeight          = makeShared<Config::Values::CIntValue>("plugin:proscenio_bars:bar_height", "Height of the bar", 40, Config::Values::SIntValueOptions{.min = 1});
    config.barColor           = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:bar_color", "Color of the bar", 0xFF211F26);
    config.inactiveBarColor   = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:inactive_bar_color", "Color of the bar on an unfocused window", 0xFF211F26);
    config.titleColor         = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:title_color", "Color of the title", 0xFFE6E0E9);
    config.inactiveTitleColor = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:inactive_title_color", "Color of the title on an unfocused window", 0xFFCAC4D0);
    config.titleFont          = makeShared<Config::Values::CStringValue>("plugin:proscenio_bars:title_font", "Font of the title", "Google Sans");
    config.titleSize          = makeShared<Config::Values::CIntValue>("plugin:proscenio_bars:title_size", "Size of the title", 14, Config::Values::SIntValueOptions{.min = 1});
    config.titleWeight    = makeShared<Config::Values::CFontWeightValue>("plugin:proscenio_bars:title_weight", "Weight of the title (e.g. \"bold\" or an integer 100-1000)", 500);
    config.buttonSide     = makeShared<Config::Values::CStringValue>("plugin:proscenio_bars:button_side", "Side of the bar the buttons sit on: right or left", "right");
    config.windows        = makeShared<Config::Values::CStringValue>("plugin:proscenio_bars:windows", "Windows that get a bar: floating or all", "floating");
    config.closeColor     = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:close_color", "Color of the close button", 0xFFF2B8B5);
    config.closeIconColor = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:close_icon_color", "Color of the close icon and its state layer", 0xFF601410);
    config.minimizeColor  = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:minimize_color", "Color of the minimize button", 0xFFEFB8C8);
    config.minimizeIconColor   = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:minimize_icon_color", "Color of the minimize icon and its state layer", 0xFF492532);
    config.maximizeColor       = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:maximize_color", "Color of the maximize button", 0xFFD0BCFF);
    config.maximizeIconColor   = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:maximize_icon_color", "Color of the maximize icon and its state layer", 0xFF381E72);
    config.inactiveButtonColor = makeShared<Config::Values::CColorValue>("plugin:proscenio_bars:inactive_button_color", "Color of the buttons on an unfocused window", 0xFF36343B);
    config.scrollingPeek       = makeShared<Config::Values::CIntValue>("plugin:proscenio_bars:scrolling_peek", "Pixels of the neighboring columns shown while scrolling", 40,
                                                                       Config::Values::SIntValueOptions{.min = 0, .max = 400});

    HyprlandAPI::addConfigValueV2(PHANDLE, config.barHeight);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.barColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.inactiveBarColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.inactiveTitleColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleFont);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleSize);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.titleWeight);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.buttonSide);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.windows);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.closeColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.closeIconColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.minimizeColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.minimizeIconColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.maximizeColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.maximizeIconColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.inactiveButtonColor);
    HyprlandAPI::addConfigValueV2(PHANDLE, config.scrollingPeek);

    HyprlandAPI::registerHyprCtlCommand(PHANDLE, {.name = BAR_DISPLAY_NAME, .handler = listWindowsWithBar});
    HyprlandAPI::addLuaFunction(PHANDLE, BAR_DISPLAY_NAME, "toggle_scrolling", toggleScrollingFromLua);

    for (const auto& window : Desktop::windowState()->windows()) {
        if (validMapped(window))
            onNewWindow(window);
    }

    HyprlandAPI::reloadConfig();

    return {"proscenio_bars", "Title bars for windows that ask for one.", "fEst", "1.0"};
}

APICALL EXPORT void PLUGIN_EXIT() {
    for (const auto& monitor : State::monitorState()->monitors())
        monitor->m_scheduledRecalc = true;

    g_pHyprRenderer->context().m_pass.removeAllOfType("CTitleBarPassElement");

    g_pGlobalState->iconTextures.clear();
    disableScrollingRules();
    g_pGlobalState->clickCenterLock.reset();

    Desktop::Rule::windowEffects()->unregisterEffect(g_pGlobalState->noBarRuleIdx);
    Desktop::Rule::windowEffects()->unregisterEffect(g_pGlobalState->forceBarRuleIdx);
}
