#!/usr/bin/env bash
set -euo pipefail

MAX=${NEXORA_BENCH_MAX:-${1:-10000}}

echo "Running benchmarks with NEXORA_BENCH_MAX=$MAX"
NEXORA_BENCH_MAX="$MAX" cargo bench --bench graph_bench "${@:2}"
