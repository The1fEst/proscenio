#include "../ColumnWidth.hpp"
#include "Expect.hpp"

#include <cmath>
#include <format>
#include <vector>

int main() {
    struct SCase {
        const char*         name;
        SColumnWidthRequest request;
        long                expectedPixels;
    };

    const std::vector<SCase> CASES = {
        {"without gaps the peek is left at each edge", {.viewportSize = 1920, .outerGaps = 0, .innerGaps = 0, .peek = 20}, 1880},
        {"gaps between columns and at the screen edges", {.viewportSize = 1920, .outerGaps = 20, .innerGaps = 10, .peek = 20}, 1870},
        {"no peek and no gaps fill the viewport", {.viewportSize = 2560, .outerGaps = 0, .innerGaps = 0, .peek = 0}, 2560},
        {"outer gaps wider than the rest clamp to the usable size", {.viewportSize = 1920, .outerGaps = 60, .innerGaps = 10, .peek = 0}, 1860},
        {"a peek wider than the viewport clamps to the narrowest column", {.viewportSize = 1000, .outerGaps = 0, .innerGaps = 0, .peek = 499}, 50},
    };

    for (const auto& testCase : CASES) {
        const double USABLE_SIZE = testCase.request.viewportSize - testCase.request.outerGaps;
        const long   PIXELS      = std::lround(computePeekColumnWidth(testCase.request) * USABLE_SIZE);
        expect(PIXELS == testCase.expectedPixels, std::format("peek column width: {}: {}", testCase.name, PIXELS));
    }

    const float FULL_WIDTH = computePeekColumnWidth({.viewportSize = 10, .outerGaps = 20, .innerGaps = 0, .peek = 20});
    expect(FULL_WIDTH == 1.F, std::format("peek column width: no usable size gives a full column: {}", FULL_WIDTH));

    return reportResult("ColumnWidthTests");
}
