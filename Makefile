BIN=glidevk
LIB=glide3x
DEBUG_TARGET=debug
TESTS=tests
ARCH?=i686
TARGET_ARCH=$(ARCH)-pc-windows-gnu
MINGW_ARCH=$(ARCH)-w64-mingw32-gcc
PROJ_DIR=$(shell pwd)

# Set RELEASE_TYPE to 'release' for optimized build, 'debug' for debug build
RELEASE_TYPE ?= release

ifeq ($(RELEASE_TYPE),release)
    RELEASE_FLAG = --release
    BUILD_DIR = release
else
    RELEASE_FLAG =
    BUILD_DIR = debug
endif

.PHONY: shaders
shaders:
	cd shaders && bash compile.sh

.PHONY: build
build: shaders
	cargo build --workspace $(RELEASE_FLAG)

.PHONY: build-win
build-win: shaders
	cross build --workspace $(RELEASE_FLAG) --target $(TARGET_ARCH)
# ifeq ($(RELEASE_TYPE),release)
	cp target/$(TARGET_ARCH)/$(BUILD_DIR)/$(LIB).dll $(TESTS)/
# else
# 	$(MINGW_ARCH:gcc=strip) --strip-debug target/$(TARGET_ARCH)/$(BUILD_DIR)/$(LIB).dll -o $(TESTS)/$(LIB).dll
# endif

.PHONY: test
test: build
	gcc -Wall -g -O0 tests/example.c -Itests -L$(DEBUG_TARGET) -l$(LIB) -o $(DEBUG_TARGET)/example
	@LD_LIBRARY_PATH=$(DEBUG_TARGET) $(DEBUG_TARGET)/example

.PHONY: run-test
run-test:
	cargo run --bin=test --package=glide3x

EXAMPLE=example
EXAMPLE_BIN=$(TESTS)/$(EXAMPLE)

.PHONY: build-example
build-example:
	$(MINGW_ARCH) -L $(TESTS)/ -o $(EXAMPLE_BIN).exe $(EXAMPLE_BIN).c -l$(LIB)

.PHONY: run-example
run-example: build-win build-example
	wine $(EXAMPLE_BIN).exe
