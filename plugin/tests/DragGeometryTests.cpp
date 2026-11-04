#include "../DragGeometry.hpp"
#include "Expect.hpp"

#include <format>
#include <vector>

int main() {
    struct SCase {
        const char*     name;
        SRestoreRequest request;
        SWindowPosition expected;
    };

    const std::vector<SCase> CASES = {
        {"grab at the bar's left edge", {.pointerX = 10, .pointerY = 25, .grabX = 0, .grabY = 20, .barWidth = 1920, .barOffsetY = 40, .restoredWidth = 1400}, {10, 45}},
        {"grab in the middle", {.pointerX = 960, .pointerY = 25, .grabX = 960, .grabY = 20, .barWidth = 1920, .barOffsetY = 40, .restoredWidth = 1400}, {260, 45}},
        {"grab near the right edge", {.pointerX = 1800, .pointerY = 30, .grabX = 1800, .grabY = 10, .barWidth = 1920, .barOffsetY = 40, .restoredWidth = 800}, {1050, 60}},
        {"pointer already dragged down", {.pointerX = 800, .pointerY = 270, .grabX = 800, .grabY = 20, .barWidth = 1920, .barOffsetY = 40, .restoredWidth = 1400}, {217, 290}},
        {"pointer past the grab point", {.pointerX = 700, .pointerY = 4, .grabX = 480, .grabY = 2, .barWidth = 960, .barOffsetY = 30, .restoredWidth = 400}, {500, 32}},
        {"window off the top is not clamped", {.pointerX = 50, .pointerY = 0, .grabX = 100, .grabY = 39, .barWidth = 1000, .barOffsetY = 40, .restoredWidth = 1000}, {-50, 1}},
    };

    for (const auto& testCase : CASES)
        expect(computeRestoredPosition(testCase.request) == testCase.expected, std::format("restored position: {}", testCase.name));

    return reportResult("DragGeometryTests");
}
