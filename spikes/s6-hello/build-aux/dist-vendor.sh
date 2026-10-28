#!/bin/sh
# Called by `meson dist`: vendors crates into the release tarball so Flathub can build offline.
set -eu
export DIST="$1"
export SOURCE_ROOT="$2"

cd "$SOURCE_ROOT"
mkdir "$DIST"/.cargo
# cargo-vendor-filterer ships with the rust-stable SDK extension; it keeps only Linux crates.
cargo vendor-filterer --platform=x86_64-unknown-linux-gnu --platform=aarch64-unknown-linux-gnu > "$DIST"/.cargo/config.toml
sed -i 's/^directory = ".*"/directory = "vendor"/g' "$DIST"/.cargo/config.toml
# Move vendor into dist tarball directory
mv vendor "$DIST"
