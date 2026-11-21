#pragma once

#include <hyprland/src/desktop/DesktopTypes.hpp>

#include "LayoutSnapshot.hpp"

#include <variant>
#include <vector>

struct lua_State;

namespace Layout {
    class ITarget;
}

struct SArrangementSnapshot {
    std::vector<WP<Layout::ITarget>>                                    targets;
    std::variant<std::monostate, SDwindleTree, std::vector<SMasterSlot>> arrangement;
};

bool isScrollingToggled(const PHLWORKSPACE& workspace);
void toggleScrollingLayout(const PHLWORKSPACE& workspace);
void disableScrollingRules();
int  toggleScrollingFromLua(lua_State* state);
