# Build the compose_rspaced CLI. This Makefile only wraps cargo.
#
#   make            # debug build
#   make release    # optimized build
#   make install    # install release binary to $(PREFIX)/bin
#   make test       # cargo test
#   make fmt        # cargo fmt
#   make clippy     # cargo clippy (warnings = errors)
#   make clean      # cargo clean

CARGO    ?= cargo
PREFIX   ?= /usr/local
BIN_NAME  = compose_rspaced

.PHONY: build release install test fmt clippy clean

build:
	$(CARGO) build

release:
	$(CARGO) build --release

install: release
	install -m 0755 target/release/$(BIN_NAME) $(PREFIX)/bin/$(BIN_NAME)

test:
	$(CARGO) test

fmt:
	$(CARGO) fmt

clippy:
	$(CARGO) clippy -- -D warnings

clean:
	$(CARGO) clean
