#pragma once

#include <string_view>

constexpr std::string_view SCROLLING_LAYOUT = "scrolling";
constexpr std::string_view MONOCLE_LAYOUT   = "monocle";

std::string_view parseToggleLayout(std::string_view option);
