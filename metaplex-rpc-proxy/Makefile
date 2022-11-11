# a small optimized binary without debug info, useful for releases
build: clean
	wasm-pack build --release

# a large binary with debug info and no optimizations, useful
# while debugging
build-unoptimized: clean
	cargo +nightly build --target=wasm32-unknown-unknown --release

clean:
	cargo clean
	rm -rf ./pkg
