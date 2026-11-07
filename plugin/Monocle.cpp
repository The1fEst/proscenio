#define WLR_USE_UNSTABLE

#include "Monocle.hpp"

#include <hyprland/src/config/lua/bindings/LuaBindingsInternal.hpp>
#include <hyprland/src/config/shared/workspace/WorkspaceRuleManager.hpp>
#include <hyprland/src/config/supplementary/propRefresher/PropRefresher.hpp>
#include <hyprland/src/desktop/state/FocusState.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/layout/algorithm/Algorithm.hpp>
#include <hyprland/src/layout/space/Space.hpp>
#include <hyprland/src/layout/supplementary/WorkspaceAlgoMatcher.hpp>
#include <hyprland/src/workspace/HLWorkspace.hpp>

#include "globals.hpp"

constexpr auto     MONOCLE_LAYOUT = "monocle";

static std::string formatWorkspaceSelector(const PHLWORKSPACE& workspace) {
    const auto ID = workspace->id();
    if (const auto* NUMBERED_ID = std::get_if<Workspace::SWorkspaceNumberedID>(&ID))
        return std::to_string(NUMBERED_ID->value);

    if (workspace->type() == Workspace::eWorkspaceType::SPECIAL)
        return workspace->addressableName();

    return "name:" + workspace->addressableName();
}

bool isMonocleLayout(const PHLWORKSPACE& workspace) {
    if (!workspace)
        return false;

    const auto SPACE = workspace->space();
    if (!SPACE || !SPACE->algorithm() || !SPACE->algorithm()->tiledAlgo())
        return false;

    return Layout::Supplementary::algoMatcher()->getNameForTiledAlgo(SPACE->algorithm()->tiledAlgo().get()) == MONOCLE_LAYOUT;
}

void toggleMonocleLayout(const PHLWORKSPACE& workspace) {
    if (!workspace)
        return;

    const auto SELECTOR = formatWorkspaceSelector(workspace);

    if (const auto RULE = g_pGlobalState->monocleRules[SELECTOR].lock())
        RULE->setEnabled(!RULE->isEnabled());
    else {
        Config::CWorkspaceRule monocleRule;
        monocleRule.m_workspaceString = SELECTOR;
        monocleRule.m_layout          = MONOCLE_LAYOUT;
        monocleRule.setEnabled(!isMonocleLayout(workspace));

        g_pGlobalState->monocleRules[SELECTOR] = Config::workspaceRuleMgr()->add(std::move(monocleRule));
    }

    Config::Supplementary::refresher()->scheduleRefresh(Config::Supplementary::REFRESH_MONITOR_STATES | Config::Supplementary::REFRESH_WINDOW_STATES);
}

void disableMonocleRules() {
    for (const auto& [selector, rule] : g_pGlobalState->monocleRules) {
        if (const auto RULE = rule.lock())
            RULE->setEnabled(false);
    }

    g_pGlobalState->monocleRules.clear();
    Config::Supplementary::refresher()->scheduleRefresh(Config::Supplementary::REFRESH_MONITOR_STATES | Config::Supplementary::REFRESH_WINDOW_STATES);
}

int toggleMonocleFromLua(lua_State* state) {
    PHLWORKSPACE workspace;

    if (lua_isnoneornil(state, 1)) {
        const auto MONITOR = Desktop::focusState()->monitor();
        if (MONITOR)
            workspace = MONITOR->m_activeSpecialWorkspace ? MONITOR->m_activeSpecialWorkspace : MONITOR->m_activeWorkspace;
    } else
        workspace = Config::Lua::Bindings::Internal::workspaceFromLuaSelectorOrObject(state, 1, "toggle_monocle");

    if (!workspace)
        return Config::Lua::Bindings::Internal::configError(state, "toggle_monocle: no such workspace");

    toggleMonocleLayout(workspace);
    return 0;
}
