#pragma once

#include <hyprland/src/desktop/DesktopTypes.hpp>

#include "LayoutSnapshot.hpp"

#include <variant>
#include <vector>

struct lua_State;

namespace Layout {
    class ITarget;
}

struct SMonocleSnapshot {
    std::vector<WP<Layout::ITarget>>                                                                     targets;
    std::variant<std::monostate, SDwindleTree, std::vector<SMasterSlot>, std::vector<SScrollingColumn>> arrangement;
};

bool isMonocleLayout(const PHLWORKSPACE& workspace);
void toggleMonocleLayout(const PHLWORKSPACE& workspace);
void disableMonocleRules();
int  toggleFullscreenFromLua(lua_State* state);
