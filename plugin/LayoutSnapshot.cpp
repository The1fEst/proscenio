#include "LayoutSnapshot.hpp"

#include <algorithm>
#include <cmath>
#include <numeric>

static bool isPresentTarget(const std::vector<bool>& isPresent, size_t target) {
    return target < isPresent.size() && isPresent[target];
}

static std::optional<size_t> copyPresentSubtree(const SDwindleTree& snapshot, size_t index, const std::vector<bool>& isPresent, std::vector<SDwindleNode>& nodes, size_t depth) {
    if (index >= snapshot.nodes.size() || depth > snapshot.nodes.size())
        return std::nullopt;

    const auto& NODE = snapshot.nodes[index];
    if (NODE.target) {
        if (!isPresentTarget(isPresent, *NODE.target))
            return std::nullopt;

        nodes.push_back({.target = NODE.target});
        return nodes.size() - 1;
    }

    const auto FIRST  = copyPresentSubtree(snapshot, NODE.children[0], isPresent, nodes, depth + 1);
    const auto SECOND = copyPresentSubtree(snapshot, NODE.children[1], isPresent, nodes, depth + 1);
    if (!FIRST || !SECOND)
        return FIRST ? FIRST : SECOND;

    nodes.push_back({.children = {*FIRST, *SECOND}, .isSplitTop = NODE.isSplitTop, .splitRatio = NODE.splitRatio});
    return nodes.size() - 1;
}

SDwindleTree planDwindleTree(const SDwindleTree& snapshot, const std::vector<bool>& isPresent, const std::vector<size_t>& newTargets) {
    SDwindleTree plan;
    const auto   ROOT = copyPresentSubtree(snapshot, snapshot.root, isPresent, plan.nodes, 0);
    if (ROOT)
        plan.root = *ROOT;

    for (const auto TARGET : newTargets) {
        if (plan.nodes.empty()) {
            plan.nodes.push_back({.target = TARGET});
            plan.root = 0;
            continue;
        }

        size_t lastLeaf         = plan.root;
        bool   isParentSplitTop = true;
        while (!plan.nodes[lastLeaf].target) {
            isParentSplitTop = plan.nodes[lastLeaf].isSplitTop;
            lastLeaf         = plan.nodes[lastLeaf].children[1];
        }

        plan.nodes.push_back({.target = plan.nodes[lastLeaf].target});
        plan.nodes.push_back({.target = TARGET});
        plan.nodes[lastLeaf] = {.children = {plan.nodes.size() - 2, plan.nodes.size() - 1}, .isSplitTop = !isParentSplitTop};
    }

    return plan;
}

std::vector<size_t> sortMasterSlots(const std::vector<SMasterSlotPosition>& positions) {
    std::vector<size_t> order(positions.size());
    std::iota(order.begin(), order.end(), 0);
    std::ranges::stable_sort(order, [&positions](size_t a, size_t b) {
        const auto& A = positions[a];
        const auto& B = positions[b];
        if (A.isMaster != B.isMaster)
            return A.isMaster;
        if (std::lround(A.x) != std::lround(B.x))
            return std::lround(A.x) < std::lround(B.x);
        return std::lround(A.y) < std::lround(B.y);
    });
    return order;
}

std::vector<SMasterSlot> planMasterSlots(const std::vector<SMasterSlot>& snapshot, const std::vector<bool>& isPresent, const std::vector<size_t>& newTargets) {
    std::vector<SMasterSlot> plan;
    for (const auto& SLOT : snapshot) {
        if (isPresentTarget(isPresent, SLOT.target))
            plan.push_back(SLOT);
    }

    const float PERC_MASTER = plan.empty() ? SMasterSlot{}.percMaster : plan.front().percMaster;
    for (const auto TARGET : newTargets)
        plan.push_back({.target = TARGET, .percMaster = PERC_MASTER});

    if (!plan.empty() && std::ranges::none_of(plan, &SMasterSlot::isMaster))
        plan.front().isMaster = true;

    return plan;
}
