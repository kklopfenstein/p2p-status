default: build
pi:
	export TARGET=arm-unknown-linux-gnueabihf
	cargo build --target arm-unknown-linux-gnueabihf
build:
	cargo build