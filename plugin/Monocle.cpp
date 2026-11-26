#define WLR_USE_UNSTABLE

#include "Monocle.hpp"

#include <hyprland/src/config/lua/bindings/LuaBindingsInternal.hpp>
#include <hyprland/src/config/shared/workspace/WorkspaceRuleManager.hpp>
#include <hyprland/src/config/supplementary/propRefresher/PropRefresher.hpp>
#include <hyprland/src/desktop/state/FocusState.hpp>
#include <hyprland/src/desktop/state/WindowState.hpp>
#include <hyprland/src/desktop/view/window/Window.hpp>
#include <hyprland/src/output/Monitor.hpp>
#include <hyprland/src/layout/algorithm/Algorithm.hpp>
#include <hyprland/src/layout/algorithm/tiled/dwindle/DwindleAlgorithm.hpp>
#include <hyprland/src/layout/algorithm/tiled/master/MasterAlgorithm.hpp>
#include <hyprland/src/layout/algorithm/tiled/scrolling/ScrollingAlgorithm.hpp>
#include <hyprland/src/layout/space/Space.hpp>
#include <hyprland/src/layout/supplementary/WorkspaceAlgoMatcher.hpp>
#include <hyprland/src/layout/target/Target.hpp>
#include <hyprland/src/workspace/HLWorkspace.hpp>

#include <algorithm>
#include <map>
#include <set>
#include <utility>

#include "globals.hpp"

constexpr auto MONOCLE_LAYOUT = "monocle";

using Layout::ITarget;
using Layout::Tiled::CDwindleAlgorithm;
using Layout::Tiled::CMasterAlgorithm;
using Layout::Tiled::CScrollingAlgorithm;
using Layout::Tiled::SDwindleNodeData;

static std::string formatWorkspaceSelector(const PHLWORKSPACE& workspace) {
    const auto ID = workspace->id();
    if (const auto* NUMBERED_ID = std::get_if<Workspace::SWorkspaceNumberedID>(&ID))
        return std::to_string(NUMBERED_ID->value);

    if (workspace->type() == Workspace::eWorkspaceType::SPECIAL)
        return workspace->addressableName();

    return "name:" + workspace->addressableName();
}

static Layout::ITiledAlgorithm* findTiledAlgorithm(const PHLWORKSPACE& workspace) {
    if (!workspace)
        return nullptr;

    const auto SPACE = workspace->space();
    if (!SPACE || !SPACE->algorithm())
        return nullptr;

    return SPACE->algorithm()->tiledAlgo().get();
}

bool isMonocleLayout(const PHLWORKSPACE& workspace) {
    const auto* ALGORITHM = findTiledAlgorithm(workspace);
    return ALGORITHM && Layout::Supplementary::algoMatcher()->getNameForTiledAlgo(ALGORITHM) == MONOCLE_LAYOUT;
}

static std::vector<SP<ITarget>> collectTiledTargets(const PHLWORKSPACE& workspace) {
    std::vector<SP<ITarget>> targets;
    for (const auto& window : Desktop::windowState()->windows()) {
        if (!validMapped(window) || window->m_workspace != workspace || window->isFloating())
            continue;

        const auto TARGET = window->layoutTarget();
        if (TARGET && std::ranges::find(targets, TARGET) == targets.end())
            targets.push_back(TARGET);
    }
    return targets;
}

static SP<SDwindleNodeData> findDwindleLeaf(CDwindleAlgorithm& dwindle, const SP<ITarget>& target) {
    const auto NODE = target ? dwindle.getNodeFromWindow(target->window()) : nullptr;
    return NODE && NODE->pTarget.lock() == target ? NODE : nullptr;
}

static std::optional<size_t> captureDwindleNode(const SP<SDwindleNodeData>& node, const std::map<ITarget*, size_t>& keys, SDwindleTree& tree, size_t depth) {
    if (!node || depth > 2 * keys.size())
        return std::nullopt;

    const auto FIRST = node->children[0].lock();
    if (!FIRST) {
        const auto KEY = keys.find(node->pTarget.lock().get());
        if (KEY == keys.end())
            return std::nullopt;

        tree.nodes.push_back({.target = KEY->second});
        return tree.nodes.size() - 1;
    }

    const auto FIRST_INDEX  = captureDwindleNode(FIRST, keys, tree, depth + 1);
    const auto SECOND_INDEX = captureDwindleNode(node->children[1].lock(), keys, tree, depth + 1);
    if (!FIRST_INDEX || !SECOND_INDEX)
        return std::nullopt;

    tree.nodes.push_back({.children = {*FIRST_INDEX, *SECOND_INDEX}, .isSplitTop = node->splitTop, .splitRatio = node->splitRatio});
    return tree.nodes.size() - 1;
}

static std::optional<SDwindleTree> captureDwindleTree(CDwindleAlgorithm& dwindle, const std::vector<SP<ITarget>>& targets) {
    std::map<ITarget*, size_t> keys;
    for (size_t i = 0; i < targets.size(); ++i)
        keys[targets[i].get()] = i;

    auto root = targets.empty() ? nullptr : findDwindleLeaf(dwindle, targets.front());
    for (size_t depth = 0; root && root->pParent && depth <= 2 * targets.size(); ++depth)
        root = root->pParent.lock();

    SDwindleTree tree;
    const auto   ROOT = captureDwindleNode(root, keys, tree, 0);
    if (!ROOT || tree.nodes.size() != 2 * targets.size() - 1)
        return std::nullopt;

    tree.root = *ROOT;
    return tree;
}

static void applyDwindleTree(CDwindleAlgorithm& dwindle, const SDwindleTree& plan, const std::vector<SP<ITarget>>& keyedTargets) {
    if (plan.root >= plan.nodes.size())
        return;

    std::vector<SP<SDwindleNodeData>> leaves;
    std::set<SP<SDwindleNodeData>>    internalNodes;
    for (const auto& PLAN_NODE : plan.nodes) {
        if (!PLAN_NODE.target)
            continue;

        const auto LEAF = *PLAN_NODE.target < keyedTargets.size() ? findDwindleLeaf(dwindle, keyedTargets[*PLAN_NODE.target]) : nullptr;
        if (!LEAF)
            return;

        leaves.push_back(LEAF);
        auto parent = LEAF->pParent.lock();
        for (size_t depth = 0; parent && depth < plan.nodes.size(); ++depth) {
            internalNodes.insert(parent);
            parent = parent->pParent.lock();
        }
    }

    if (leaves.size() + internalNodes.size() != plan.nodes.size())
        return;

    std::vector<SP<SDwindleNodeData>> nodesByPlanIndex(plan.nodes.size());
    auto                              nextLeaf     = leaves.begin();
    auto                              nextInternal = internalNodes.begin();
    for (size_t i = 0; i < plan.nodes.size(); ++i)
        nodesByPlanIndex[i] = plan.nodes[i].target ? *nextLeaf++ : *nextInternal++;

    nodesByPlanIndex[plan.root]->pParent.reset();
    for (size_t i = 0; i < plan.nodes.size(); ++i) {
        const auto& PLAN_NODE = plan.nodes[i];
        const auto& NODE      = nodesByPlanIndex[i];
        NODE->isNode          = !PLAN_NODE.target;
        if (PLAN_NODE.target) {
            NODE->children = {};
            continue;
        }

        NODE->splitTop   = PLAN_NODE.isSplitTop;
        NODE->splitRatio = PLAN_NODE.splitRatio;
        for (size_t side = 0; side < 2; ++side) {
            const auto& CHILD    = nodesByPlanIndex[PLAN_NODE.children[side]];
            NODE->children[side] = CHILD;
            CHILD->pParent       = NODE;
        }
    }

    dwindle.recalculate();
}

static std::vector<SP<Layout::Tiled::SMasterNodeData>> findMasterNodes(CMasterAlgorithm& master, const std::vector<SP<ITarget>>& targets) {
    std::vector<SP<Layout::Tiled::SMasterNodeData>> nodes;
    for (const auto& TARGET : targets) {
        const auto NODE = master.getNodeFromTarget(TARGET);
        if (!NODE)
            return {};
        nodes.push_back(NODE);
    }
    return nodes;
}

static std::vector<size_t> sortMasterNodes(const std::vector<SP<Layout::Tiled::SMasterNodeData>>& nodes) {
    std::vector<SMasterSlotPosition> positions;
    for (const auto& NODE : nodes)
        positions.push_back({.isMaster = NODE->isMaster, .x = NODE->position.x, .y = NODE->position.y});
    return sortMasterSlots(positions);
}

static std::optional<std::vector<SMasterSlot>> captureMasterSlots(CMasterAlgorithm& master, const std::vector<SP<ITarget>>& targets) {
    const auto NODES = findMasterNodes(master, targets);
    if (NODES.empty())
        return std::nullopt;

    std::vector<SMasterSlot> slots;
    for (const auto INDEX : sortMasterNodes(NODES))
        slots.push_back({.target = INDEX, .isMaster = NODES[INDEX]->isMaster, .percMaster = NODES[INDEX]->percMaster, .percSize = NODES[INDEX]->percSize});
    return slots;
}

static void applyMasterSlots(CMasterAlgorithm& master, const std::vector<SMasterSlot>& plan, const std::vector<SP<ITarget>>& keyedTargets) {
    std::vector<SP<ITarget>> targets;
    for (const auto& SLOT : plan) {
        if (SLOT.target >= keyedTargets.size() || !keyedTargets[SLOT.target])
            return;
        targets.push_back(keyedTargets[SLOT.target]);
    }

    const auto NODES = findMasterNodes(master, targets);
    if (NODES.empty() || NODES.size() != plan.size())
        return;

    const auto MASTER_COUNT = std::ranges::count_if(plan, &SMasterSlot::isMaster);
    for (size_t i = 0; i < NODES.size(); ++i)
        NODES[i]->isMaster = std::cmp_less(i, MASTER_COUNT);
    master.recalculate();

    const auto ORDER = sortMasterNodes(NODES);
    for (size_t i = 0; i < ORDER.size(); ++i) {
        const auto& NODE = NODES[ORDER[i]];
        NODE->pTarget    = targets[i];
        NODE->percMaster = plan[i].percMaster;
        NODE->percSize   = plan[i].percSize;
    }
    master.recalculate();
}

static SP<Layout::Tiled::SScrollingData> findScrollingData(CScrollingAlgorithm& scrolling, const std::vector<SP<ITarget>>& targets) {
    const auto DATA   = targets.empty() ? nullptr : scrolling.dataFor(targets.front());
    const auto COLUMN = DATA ? DATA->column.lock() : nullptr;
    return COLUMN ? COLUMN->scrollingData.lock() : nullptr;
}

static std::optional<std::vector<SScrollingColumn>> captureScrollingColumns(CScrollingAlgorithm& scrolling, const std::vector<SP<ITarget>>& targets) {
    const auto SCROLLING_DATA = findScrollingData(scrolling, targets);
    if (!SCROLLING_DATA)
        return std::nullopt;

    std::vector<SScrollingColumn> columns;
    for (const auto& COLUMN : SCROLLING_DATA->columns) {
        if (!COLUMN)
            continue;

        SScrollingColumn column = {.width = COLUMN->getColumnWidth()};
        for (size_t i = 0; i < COLUMN->targetDatas.size(); ++i) {
            const auto TARGET = COLUMN->targetDatas[i] ? COLUMN->targetDatas[i]->target.lock() : nullptr;
            const auto KEY    = std::ranges::find(targets, TARGET);
            if (!TARGET || KEY == targets.end())
                continue;

            column.targets.push_back(KEY - targets.begin());
            column.sizes.push_back(COLUMN->getTargetSize(i));
        }
        columns.push_back(std::move(column));
    }
    return columns;
}

static void applyScrollingColumns(CScrollingAlgorithm& scrolling, const std::vector<SScrollingColumn>& plan, const std::vector<SP<ITarget>>& keyedTargets) {
    std::vector<std::vector<SP<Layout::Tiled::SScrollingTargetData>>> targetDataByColumn;
    std::vector<SP<ITarget>>                                          targets;
    for (const auto& COLUMN : plan) {
        auto& columnTargetData = targetDataByColumn.emplace_back();
        for (const auto KEY : COLUMN.targets) {
            const auto TARGET = KEY < keyedTargets.size() ? keyedTargets[KEY] : nullptr;
            const auto DATA   = TARGET ? scrolling.dataFor(TARGET) : nullptr;
            if (!DATA || !DATA->column.lock())
                return;
            columnTargetData.push_back(DATA);
            targets.push_back(TARGET);
        }
    }

    const auto SCROLLING_DATA = findScrollingData(scrolling, targets);
    if (!SCROLLING_DATA)
        return;

    for (size_t i = 0; i < plan.size(); ++i) {
        const auto COLUMN = SCROLLING_DATA->add(plan[i].width);
        for (const auto& DATA : targetDataByColumn[i]) {
            if (const auto OLD_COLUMN = DATA->column.lock())
                OLD_COLUMN->remove(DATA->target.lock());
            COLUMN->add(DATA);
        }

        for (size_t row = 0; row < plan[i].sizes.size(); ++row)
            COLUMN->setTargetSize(row, plan[i].sizes[row]);
    }

    scrolling.recalculate();
}

static SMonocleSnapshot captureArrangement(const PHLWORKSPACE& workspace) {
    SMonocleSnapshot snapshot;
    auto*            algorithm = findTiledAlgorithm(workspace);
    const auto       TARGETS   = collectTiledTargets(workspace);
    if (!algorithm || TARGETS.empty())
        return snapshot;

    for (const auto& TARGET : TARGETS)
        snapshot.targets.emplace_back(TARGET);

    if (auto* dwindle = dynamic_cast<CDwindleAlgorithm*>(algorithm)) {
        if (auto tree = captureDwindleTree(*dwindle, TARGETS))
            snapshot.arrangement = std::move(*tree);
    } else if (auto* master = dynamic_cast<CMasterAlgorithm*>(algorithm)) {
        if (auto slots = captureMasterSlots(*master, TARGETS))
            snapshot.arrangement = std::move(*slots);
    } else if (auto* scrolling = dynamic_cast<CScrollingAlgorithm*>(algorithm)) {
        if (auto columns = captureScrollingColumns(*scrolling, TARGETS))
            snapshot.arrangement = std::move(*columns);
    }

    return snapshot;
}

static void restoreArrangement(const PHLWORKSPACE& workspace, const SMonocleSnapshot& snapshot) {
    auto* algorithm = findTiledAlgorithm(workspace);
    if (!algorithm || std::holds_alternative<std::monostate>(snapshot.arrangement))
        return;

    std::vector<SP<ITarget>> keyedTargets(snapshot.targets.size());
    std::vector<bool>        isPresent(snapshot.targets.size(), false);
    std::vector<size_t>      newTargets;
    for (const auto& TARGET : collectTiledTargets(workspace)) {
        const auto SNAPSHOT_TARGET = std::ranges::find_if(snapshot.targets, [&TARGET](const auto& snapshotTarget) { return snapshotTarget.lock() == TARGET; });
        if (SNAPSHOT_TARGET != snapshot.targets.end()) {
            const size_t KEY  = SNAPSHOT_TARGET - snapshot.targets.begin();
            keyedTargets[KEY] = TARGET;
            isPresent[KEY]    = true;
            continue;
        }

        newTargets.push_back(keyedTargets.size());
        keyedTargets.push_back(TARGET);
    }

    if (const auto* TREE = std::get_if<SDwindleTree>(&snapshot.arrangement)) {
        if (auto* dwindle = dynamic_cast<CDwindleAlgorithm*>(algorithm))
            applyDwindleTree(*dwindle, planDwindleTree(*TREE, isPresent, newTargets), keyedTargets);
    } else if (const auto* SLOTS = std::get_if<std::vector<SMasterSlot>>(&snapshot.arrangement)) {
        if (auto* master = dynamic_cast<CMasterAlgorithm*>(algorithm))
            applyMasterSlots(*master, planMasterSlots(*SLOTS, isPresent, newTargets), keyedTargets);
    } else if (const auto* COLUMNS = std::get_if<std::vector<SScrollingColumn>>(&snapshot.arrangement)) {
        if (auto* scrolling = dynamic_cast<CScrollingAlgorithm*>(algorithm))
            applyScrollingColumns(*scrolling, planScrollingColumns(*COLUMNS, isPresent, newTargets), keyedTargets);
    }
}

void toggleMonocleLayout(const PHLWORKSPACE& workspace) {
    if (!workspace)
        return;

    const auto SELECTOR    = formatWorkspaceSelector(workspace);
    const auto RULE        = g_pGlobalState->monocleRules[SELECTOR].lock();
    const bool IS_ENTERING =RULE ? !RULE->isEnabled() : !isMonocleLayout(workspace);

    if (IS_ENTERING)
        g_pGlobalState->monocleSnapshots[SELECTOR] = captureArrangement(workspace);

    if (RULE)
        RULE->setEnabled(IS_ENTERING);
    else {
        Config::CWorkspaceRule monocleRule;
        monocleRule.m_workspaceString = SELECTOR;
        monocleRule.m_layout          = MONOCLE_LAYOUT;
        monocleRule.setEnabled(IS_ENTERING);

        g_pGlobalState->monocleRules[SELECTOR] = Config::workspaceRuleMgr()->add(std::move(monocleRule));
    }

    Layout::Supplementary::algoMatcher()->updateWorkspaceLayouts();

    if (!IS_ENTERING) {
        if (const auto SNAPSHOT = g_pGlobalState->monocleSnapshots.find(SELECTOR); SNAPSHOT != g_pGlobalState->monocleSnapshots.end()) {
            restoreArrangement(workspace, SNAPSHOT->second);
            g_pGlobalState->monocleSnapshots.erase(SNAPSHOT);
        }
    }

    Config::Supplementary::refresher()->scheduleRefresh(Config::Supplementary::REFRESH_MONITOR_STATES | Config::Supplementary::REFRESH_WINDOW_STATES);
}

void disableMonocleRules() {
    for (const auto& [selector, rule] : g_pGlobalState->monocleRules) {
        if (const auto RULE = rule.lock())
            RULE->setEnabled(false);
    }

    g_pGlobalState->monocleRules.clear();
    g_pGlobalState->monocleSnapshots.clear();
    Config::Supplementary::refresher()->scheduleRefresh(Config::Supplementary::REFRESH_MONITOR_STATES | Config::Supplementary::REFRESH_WINDOW_STATES);
}

int toggleMonocleFromLua(lua_State* state) {
    PHLWORKSPACE workspace;

    if (lua_isnoneornil(state, 1)) {
        const auto MONITOR = Desktop::focusState()->monitor();
        if (MONITOR)
            workspace = MONITOR->m_activeSpecialWorkspace ? MONITOR->m_activeSpecialWorkspace : MONITOR->m_activeWorkspace;
    } else
        workspace = Config::Lua::Bindings::Internal::workspaceFromLuaSelectorOrObject(state, 1, "toggle_monocle");

    if (!workspace)
        return Config::Lua::Bindings::Internal::configError(state, "toggle_monocle: no such workspace");

    toggleMonocleLayout(workspace);
    return 0;
}
