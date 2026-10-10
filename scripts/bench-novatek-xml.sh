#!/usr/bin/env bash
set -euo pipefail
# Run from the repository root: devenv shell -- bash scripts/bench-novatek-xml.sh
root=$(pwd)
baseline=${1:-6a7f7d9d54f269f94aee0d73242770f7034e2b8c}
bench_dir=$(mktemp -d "${TMPDIR:-/tmp}/cutout-novatek-bench.XXXXXX")
trap 'rm -rf "$bench_dir"' EXIT
mkdir "$bench_dir/src"
git show "$baseline:crates/cutout-protocols/src/novatek.rs" > "$bench_dir/src/legacy.rs"
cp crates/cutout-protocols/src/novatek.rs "$bench_dir/src/current.rs"
cp scripts/novatek-xml-benchmark.rs.in "$bench_dir/src/main.rs"
cat > "$bench_dir/Cargo.toml" <<'TOML'
[package]
name = "cutout-novatek-xml-benchmark"
version = "0.0.0"
edition = "2024"
[dependencies]
arrayvec = "=0.7.7"
thiserror = "=2.0.18"
quick-xml = { version = "=0.42.0", default-features = false }
TOML
printf 'baseline=%s\n' "$baseline"
rustc --version
cargo run --release --manifest-path "$bench_dir/Cargo.toml" --target-dir "$root/target" --quiet
