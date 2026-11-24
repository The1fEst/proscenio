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
    std::vector<WP<Layout::ITarget>>                                                                     targets;
    std::variant<std::monostate, SDwindleTree, std::vector<SMasterSlot>, std::vector<SScrollingColumn>> arrangement;
};

bool isLayoutToggled(const PHLWORKSPACE& workspace);
void toggleLayout(const PHLWORKSPACE& workspace);
void resizeScrollingColumns(const PHLWORKSPACE& workspace);
void centerFocusedColumn(const PHLWORKSPACE& workspace);
void centerClickedColumn(const PHLWINDOW& window);
void disableLayoutToggleRules();
int  toggleLayoutFromLua(lua_State* state);
