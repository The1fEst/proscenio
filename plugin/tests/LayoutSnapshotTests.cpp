#include "../LayoutSnapshot.hpp"
#include "Expect.hpp"

#include <format>
#include <string>

static std::string describeDwindleNode(const SDwindleTree& tree, size_t index) {
    const auto& NODE = tree.nodes.at(index);
    if (NODE.target)
        return std::to_string(*NODE.target);

    return std::format("({} {:.2f} {} {})", NODE.isSplitTop ? "top" : "side", NODE.splitRatio, describeDwindleNode(tree, NODE.children[0]),
                       describeDwindleNode(tree, NODE.children[1]));
}

static std::string describeDwindleTree(const SDwindleTree& tree) {
    return tree.nodes.empty() ? "" : describeDwindleNode(tree, tree.root);
}

static std::string describeMasterSlots(const std::vector<SMasterSlot>& slots) {
    std::string text;
    for (const auto& SLOT : slots)
        text += std::format("{}{}:{:.2f}:{:.2f} ", SLOT.isMaster ? "M" : "S", SLOT.target, SLOT.percMaster, SLOT.percSize);
    return text;
}

static void checkDwindle() {
    const SDwindleTree REMEMBERED = {
        .nodes =
            {
                {.children = {1, 2}, .isSplitTop = false, .splitRatio = 1.2F},
                {.target = 0},
                {.children = {3, 4}, .isSplitTop = true, .splitRatio = 0.6F},
                {.target = 1},
                {.target = 2},
            },
        .root = 0,
    };

    struct SCase {
        const char*         name;
        std::vector<bool>   isPresent;
        std::vector<size_t> newTargets;
        std::string         expected;
    };

    const std::vector<SCase> CASES = {
        {"every target present keeps the tree and its ratios", {true, true, true}, {}, "(side 1.20 0 (top 0.60 1 2))"},
        {"a closed leaf lifts its sibling", {true, false, true}, {}, "(side 1.20 0 2)"},
        {"a closed subtree leaves the other half", {true, false, false}, {}, "0"},
        {"nothing present gives an empty tree", {false, false, false}, {}, ""},
        {"a new target splits the last leaf across its parent", {true, true, true}, {7}, "(side 1.20 0 (top 0.60 1 (side 1.00 2 7)))"},
        {"new targets alone build a chain", {false, false, false}, {5, 6}, "(side 1.00 5 6)"},
    };

    for (const auto& CASE : CASES) {
        const auto PLAN = planDwindleTree(REMEMBERED, CASE.isPresent, CASE.newTargets);
        expect(describeDwindleTree(PLAN) == CASE.expected, std::format("dwindle: {}: {}", CASE.name, describeDwindleTree(PLAN)));
    }

    const SDwindleTree CYCLE = {.nodes = {{.children = {0, 0}}}, .root = 0};
    expect(planDwindleTree(CYCLE, {}, {}).nodes.empty(), "dwindle: a cyclic tree yields nothing");
}

static void checkMaster() {
    const std::vector<SMasterSlotPosition> POSITIONS = {
        {.isMaster = false, .x = 960, .y = 540},
        {.isMaster = true, .x = 0, .y = 0},
        {.isMaster = false, .x = 960.4, .y = 0},
        {.isMaster = false, .x = 0, .y = 700},
    };
    expect(sortMasterSlots(POSITIONS) == std::vector<size_t>{1, 3, 2, 0}, "master: masters first, then by column and row");

    const std::vector<SMasterSlot> REMEMBERED = {
        {.target = 0, .isMaster = true, .percMaster = 0.7F},
        {.target = 1, .percMaster = 0.7F, .percSize = 1.4F},
        {.target = 2, .percMaster = 0.7F, .percSize = 0.6F},
    };

    struct SCase {
        const char*         name;
        std::vector<bool>   isPresent;
        std::vector<size_t> newTargets;
        std::string         expected;
    };

    const std::vector<SCase> CASES = {
        {"every target present keeps order and sizes", {true, true, true}, {}, "M0:0.70:1.00 S1:0.70:1.40 S2:0.70:0.60 "},
        {"a closed stack target drops out", {true, false, true}, {}, "M0:0.70:1.00 S2:0.70:0.60 "},
        {"the first target left takes over from a closed master", {false, true, true}, {}, "M1:0.70:1.40 S2:0.70:0.60 "},
        {"a new target joins the end of the stack", {true, true, true}, {9}, "M0:0.70:1.00 S1:0.70:1.40 S2:0.70:0.60 S9:0.70:1.00 "},
        {"new targets alone get a master", {false, false, false}, {4, 5}, "M4:0.50:1.00 S5:0.50:1.00 "},
    };

    for (const auto& CASE : CASES) {
        const auto PLAN = planMasterSlots(REMEMBERED, CASE.isPresent, CASE.newTargets);
        expect(describeMasterSlots(PLAN) == CASE.expected, std::format("master: {}: {}", CASE.name, describeMasterSlots(PLAN)));
    }
}

int main() {
    checkDwindle();
    checkMaster();
    return reportResult("LayoutSnapshotTests");
}
