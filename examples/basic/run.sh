#!/bin/sh
set -eu

ROFI_PREFIX="${ROFI_PREFIX:-}"

cd "$(dirname "$0")"
cargo build
cd ../..

mkdir -p "$ROFI_PREFIX"/lib/rofi
if ! cp target/debug/librofi_plugin_example_basic.so "$ROFI_PREFIX"/lib/rofi/plugin_example_basic.so
then
	echo Attempting to copy again as root
	sudo cp target/debug/librofi_plugin_example_basic.so "$ROFI_PREFIX"/lib/rofi/plugin_example_basic.so
fi

#gdb --args \
	"$ROFI_PREFIX"/bin/rofi -modi run,plugin-example-basic -show plugin-example-basic "$@"
