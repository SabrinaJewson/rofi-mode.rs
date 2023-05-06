#!/bin/sh
set -eu

cd "$(dirname "$0")"
cargo build
cd ../..

# DEBUGGER can be e.g. "gdb --args"
ROFI_PLUGIN_PATH=target/debug/librofi_plugin_example_basic.so ${DEBUGGER:-} rofi \
	-modi run,plugin-example-basic \
	-show plugin-example-basic \
	"$@"
