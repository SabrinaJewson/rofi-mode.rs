#!/bin/sh
set -eu

cd "$(dirname "$0")"
cargo build
cd ../..

# DEBUGGER can be e.g. "gdb --args"
ROFI_PLUGIN_PATH=target/debug ${DEBUGGER:-} rofi \
	-modi plugin-example-file-browser \
	-show plugin-example-file-browser \
	"$@"
