#pragma once

#include <hyprland/src/desktop/DesktopTypes.hpp>

struct lua_State;

bool isMonocleLayout(const PHLWORKSPACE& workspace);
void toggleMonocleLayout(const PHLWORKSPACE& workspace);
void disableMonocleRules();
int  toggleMonocleFromLua(lua_State* state);
