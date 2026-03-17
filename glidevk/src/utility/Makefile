BIN=glidevk
LIB=glide3x
DEBUG_TARGET=debug
TESTS=tests
ARCH?=i686
TARGET_ARCH=$(ARCH)-pc-windows-gnu
MINGW_ARCH=$(ARCH)-w64-mingw32-gcc

.PHONY: build
build:
	cargo build --workspace

.PHONY: build-win
build-win:
	cross build --workspace --release --target $(TARGET_ARCH)
	cp target/$(TARGET_ARCH)/release/$(LIB).dll $(TESTS)/

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
