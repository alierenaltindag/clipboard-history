.PHONY: all build release test check lint fmt clean install uninstall deb

all: build

build:
	cargo build --workspace

release:
	cargo build --release --workspace

test:
	cargo test --workspace --verbose

check:
	cargo check --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

install:
	./scripts/install.sh

uninstall:
	./scripts/uninstall.sh

deb:
	./packaging/deb/build-deb.sh

appimage:
	./packaging/appimage/build-appimage.sh

clean:
	cargo clean
	rm -rf target/deb target/appimage
