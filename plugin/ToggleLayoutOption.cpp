#include "ToggleLayoutOption.hpp"

std::string_view parseToggleLayout(std::string_view option) {
    return option == MONOCLE_LAYOUT ? MONOCLE_LAYOUT : SCROLLING_LAYOUT;
}
