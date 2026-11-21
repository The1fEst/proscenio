#pragma once

#include <array>
#include <cstddef>
#include <optional>
#include <vector>

struct SDwindleNode {
    std::optional<size_t> target;
    std::array<size_t, 2> children   = {};
    bool                  isSplitTop = false;
    float                 splitRatio = 1.F;
};

struct SDwindleTree {
    std::vector<SDwindleNode> nodes;
    size_t                    root = 0;
};

struct SMasterSlot {
    size_t target     = 0;
    bool   isMaster   = false;
    float  percMaster = 0.5F;
    float  percSize   = 1.F;
};

struct SMasterSlotPosition {
    bool   isMaster = false;
    double x        = 0;
    double y        = 0;
};

SDwindleTree             planDwindleTree(const SDwindleTree& snapshot, const std::vector<bool>& isPresent, const std::vector<size_t>& newTargets);
std::vector<size_t>      sortMasterSlots(const std::vector<SMasterSlotPosition>& positions);
std::vector<SMasterSlot> planMasterSlots(const std::vector<SMasterSlot>& snapshot, const std::vector<bool>& isPresent, const std::vector<size_t>& newTargets);
