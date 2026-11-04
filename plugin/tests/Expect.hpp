#pragma once

#include <cstdio>
#include <string>

inline int  g_checkCount   = 0;
inline int  g_failureCount = 0;

inline void expect(bool isPassed, const std::string& label) {
    ++g_checkCount;

    if (isPassed)
        return;

    ++g_failureCount;
    std::printf("FAILED: %s\n", label.c_str());
}

inline int reportResult(const char* suiteName) {
    std::printf("%s: %d checks, %d failed\n", suiteName, g_checkCount, g_failureCount);
    return g_failureCount == 0 ? 0 : 1;
}
