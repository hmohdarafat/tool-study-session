#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dist_dir="$project_root/dist/linux"
binary_name="tool-study-session"

cd "$project_root"
cargo build --release

mkdir -p "$dist_dir"
install -Dm755 "$project_root/target/release/$binary_name" "$dist_dir/$binary_name"

echo "Built $dist_dir/$binary_name"
