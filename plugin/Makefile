ifeq ($(shell $(CXX) --version | grep -c clang),0)
    EXTRA_FLAGS = --no-gnu-unique
else
    EXTRA_FLAGS =
endif

CXXFLAGS ?= -O2
CXXFLAGS += -std=c++23 -Wall -Wextra -Wno-missing-field-initializers

PKG_CFLAGS := $(shell pkg-config --cflags pixman-1 libdrm hyprland libinput libudev wayland-server xkbcommon)
INCLUDES = $(patsubst -I%,-isystem %,$(filter-out -I/usr/include,$(PKG_CFLAGS)))

SRC = main.cpp TitleBarDecoration.cpp TitleBarPassElement.cpp ButtonLayout.cpp ButtonStateMachine.cpp DragGeometry.cpp
HDR = globals.hpp TitleBarDecoration.hpp TitleBarPassElement.hpp ButtonLayout.hpp ButtonStateMachine.hpp DragGeometry.hpp
TARGET = proscenio_bars.so

all: $(TARGET)

$(TARGET): $(SRC) $(HDR)
	$(CXX) $(CXXFLAGS) -shared -fPIC $(LDFLAGS) $(EXTRA_FLAGS) $(INCLUDES) $(SRC) -o $@

build/ButtonLayoutTests: tests/ButtonLayoutTests.cpp tests/Expect.hpp ButtonLayout.cpp ButtonLayout.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/ButtonLayoutTests.cpp ButtonLayout.cpp -o $@

build/ButtonStateMachineTests: tests/ButtonStateMachineTests.cpp tests/Expect.hpp ButtonStateMachine.cpp ButtonStateMachine.hpp ButtonLayout.cpp ButtonLayout.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/ButtonStateMachineTests.cpp ButtonStateMachine.cpp ButtonLayout.cpp -o $@

build/DragGeometryTests: tests/DragGeometryTests.cpp tests/Expect.hpp DragGeometry.cpp DragGeometry.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/DragGeometryTests.cpp DragGeometry.cpp -o $@

test: build/ButtonLayoutTests build/ButtonStateMachineTests build/DragGeometryTests
	./build/ButtonLayoutTests
	./build/ButtonStateMachineTests
	./build/DragGeometryTests

clean:
	rm -rf $(TARGET) build

.PHONY: all test clean
