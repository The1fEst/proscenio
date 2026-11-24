#include "../ToggleLayoutOption.hpp"
#include "Expect.hpp"

#include <format>
#include <string_view>
#include <vector>

int main() {
    struct SCase {
        const char*      option;
        std::string_view expected;
    };

    const std::vector<SCase> CASES = {
        {"scrolling", SCROLLING_LAYOUT},
        {"monocle", MONOCLE_LAYOUT},
        {"", SCROLLING_LAYOUT},
        {"dwindle", SCROLLING_LAYOUT},
        {"Monocle", SCROLLING_LAYOUT},
    };

    for (const auto& CASE : CASES) {
        const auto LAYOUT = parseToggleLayout(CASE.option);
        expect(LAYOUT == CASE.expected, std::format("toggle layout: \"{}\" gives {}", CASE.option, LAYOUT));
    }

    return reportResult("ToggleLayoutOptionTests");
}
