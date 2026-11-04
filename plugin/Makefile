ifeq ($(shell $(CXX) --version | grep -c clang),0)
    EXTRA_FLAGS = --no-gnu-unique
else
    EXTRA_FLAGS =
endif

CXXFLAGS ?= -O2
CXXFLAGS += -std=c++23 -Wall -Wextra -Wno-missing-field-initializers

PKG_CFLAGS := $(shell pkg-config --cflags pixman-1 libdrm hyprland libinput libudev wayland-server xkbcommon)

# hyprpm rewrites the prefix in hyprland.pc to its own header cache, so the Hyprland paths never come from pkg-config.
HYPRLAND_PREFIX := $(shell pkg-config --variable=prefix hyprland)
DEPENDENCY_CFLAGS := $(filter-out -I$(HYPRLAND_PREFIX) -I$(HYPRLAND_PREFIX)/hyprland%,$(PKG_CFLAGS))
INCLUDES = $(patsubst -I%,-isystem %,$(DEPENDENCY_CFLAGS)) -isystem /usr/include/hyprland/protocols -isystem /usr/include/hyprland/src -isystem /usr/include/hyprland

SRC = main.cpp TitleBarDecoration.cpp TitleBarPassElement.cpp
HDR = globals.hpp TitleBarDecoration.hpp TitleBarPassElement.hpp
TARGET = proscenio_bars.so

all: $(TARGET)

$(TARGET): $(SRC) $(HDR)
	$(CXX) $(CXXFLAGS) -shared -fPIC $(LDFLAGS) $(EXTRA_FLAGS) $(INCLUDES) $(SRC) -o $@

clean:
	rm -f $(TARGET)

.PHONY: all clean
