ifeq ($(shell $(CXX) --version | grep -c clang),0)
    EXTRA_FLAGS = --no-gnu-unique
else
    EXTRA_FLAGS =
endif

CXXFLAGS ?= -O2
CXXFLAGS += -std=c++23 -Wall -Wextra -Wno-missing-field-initializers

PKG_CFLAGS := $(shell pkg-config --cflags pixman-1 libdrm hyprland libinput libudev wayland-server xkbcommon pangocairo)
INCLUDES = $(patsubst -I%,-isystem %,$(filter-out -I/usr/include,$(PKG_CFLAGS)))
PANGOCAIRO_LIBS := $(shell pkg-config --libs pangocairo)
CAIRO_CFLAGS := $(patsubst -I%,-isystem %,$(filter-out -I/usr/include,$(shell pkg-config --cflags cairo)))
CAIRO_LIBS := $(shell pkg-config --libs cairo)

SRC = main.cpp TitleBarDecoration.cpp TitleBarPassElement.cpp ButtonLayout.cpp ButtonStateMachine.cpp DragGeometry.cpp ButtonAppearance.cpp IconRasterizer.cpp BarVisibility.cpp ScrollingToggle.cpp LayoutSnapshot.cpp ColumnWidth.cpp
HDR = globals.hpp TitleBarDecoration.hpp TitleBarPassElement.hpp ButtonLayout.hpp ButtonStateMachine.hpp DragGeometry.hpp ButtonAppearance.hpp IconRasterizer.hpp BarVisibility.hpp ScrollingToggle.hpp LayoutSnapshot.hpp ColumnWidth.hpp
TARGET = proscenio_bars.so

all: $(TARGET)

$(TARGET): $(SRC) $(HDR)
	$(CXX) $(CXXFLAGS) -shared -fPIC $(LDFLAGS) $(EXTRA_FLAGS) $(INCLUDES) $(SRC) -o $@ $(PANGOCAIRO_LIBS)

build/ButtonLayoutTests: tests/ButtonLayoutTests.cpp tests/Expect.hpp ButtonLayout.cpp ButtonLayout.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/ButtonLayoutTests.cpp ButtonLayout.cpp -o $@

build/ButtonStateMachineTests: tests/ButtonStateMachineTests.cpp tests/Expect.hpp ButtonStateMachine.cpp ButtonStateMachine.hpp ButtonLayout.cpp ButtonLayout.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/ButtonStateMachineTests.cpp ButtonStateMachine.cpp ButtonLayout.cpp -o $@

build/DragGeometryTests: tests/DragGeometryTests.cpp tests/Expect.hpp DragGeometry.cpp DragGeometry.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/DragGeometryTests.cpp DragGeometry.cpp -o $@

build/ButtonAppearanceTests: tests/ButtonAppearanceTests.cpp tests/Expect.hpp ButtonAppearance.cpp ButtonAppearance.hpp ButtonStateMachine.cpp ButtonStateMachine.hpp ButtonLayout.cpp ButtonLayout.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/ButtonAppearanceTests.cpp ButtonAppearance.cpp ButtonStateMachine.cpp ButtonLayout.cpp -o $@

build/IconRasterizerTests: tests/IconRasterizerTests.cpp tests/Expect.hpp IconRasterizer.cpp IconRasterizer.hpp ButtonLayout.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) $(CAIRO_CFLAGS) tests/IconRasterizerTests.cpp IconRasterizer.cpp -o $@ $(CAIRO_LIBS)

build/BarVisibilityTests: tests/BarVisibilityTests.cpp tests/Expect.hpp BarVisibility.cpp BarVisibility.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/BarVisibilityTests.cpp BarVisibility.cpp -o $@

build/LayoutSnapshotTests: tests/LayoutSnapshotTests.cpp tests/Expect.hpp LayoutSnapshot.cpp LayoutSnapshot.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/LayoutSnapshotTests.cpp LayoutSnapshot.cpp -o $@

build/ColumnWidthTests: tests/ColumnWidthTests.cpp tests/Expect.hpp ColumnWidth.cpp ColumnWidth.hpp
	mkdir -p build
	$(CXX) $(CXXFLAGS) tests/ColumnWidthTests.cpp ColumnWidth.cpp -o $@

test: build/ButtonLayoutTests build/ButtonStateMachineTests build/DragGeometryTests build/ButtonAppearanceTests build/IconRasterizerTests build/BarVisibilityTests build/LayoutSnapshotTests build/ColumnWidthTests
	./build/ButtonLayoutTests
	./build/ButtonStateMachineTests
	./build/DragGeometryTests
	./build/ButtonAppearanceTests
	./build/IconRasterizerTests
	./build/BarVisibilityTests
	./build/LayoutSnapshotTests
	./build/ColumnWidthTests

clean:
	rm -rf $(TARGET) build

.PHONY: all test clean
