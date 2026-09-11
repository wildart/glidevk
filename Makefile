LIB=glide3x
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
build:
	cargo build --workspace $(RELEASE_FLAG)

.PHONY: build-win
build-win:
	cargo build --workspace $(RELEASE_FLAG) --target $(TARGET_ARCH)
.PHONY: test
test:
	cargo test --workspace --lib $(RELEASE_FLAG)

.PHONY: run-test
run-test:
	cargo run $(RELEASE_FLAG) --bin=test --package=glide3x
