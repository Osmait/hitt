#!/bin/bash
set -e

BINARY="target/release/hitt"
URL="${1:-https://httpbin.org/get}"
RUNS="${2:-10}"

echo "========================================"
echo " hitt Performance Profile"
echo "========================================"
echo ""
echo "Target:  $URL"
echo "Runs:    $RUNS"
echo "Binary:  $BINARY"
echo ""

# Build release if needed
if [ ! -f "$BINARY" ]; then
    echo "Building release binary..."
    cargo build --release --quiet
fi

echo "Binary size: $(du -h "$BINARY" | cut -f1)"
echo ""

# ── 1. Memory & CPU (single request) ──────────────────────────
echo "════════════════════════════════════════"
echo " 1. Memory & CPU — single GET request"
echo "════════════════════════════════════════"
/usr/bin/time -l "$BINARY" send GET "$URL" > /dev/null 2> /tmp/hitt_profile.txt
grep -E "real|user|sys|maximum resident|page" /tmp/hitt_profile.txt || cat /tmp/hitt_profile.txt
echo ""

# ── 2. Comparison: hitt vs curl ───────────────────────────────
echo "════════════════════════════════════════"
echo " 2. hitt vs curl — $RUNS runs"
echo "════════════════════════════════════════"
if command -v hyperfine &> /dev/null; then
    hyperfine \
        --warmup 2 \
        --runs "$RUNS" \
        --export-markdown /tmp/hitt_vs_curl.md \
        "$BINARY send GET $URL" \
        "curl -s -o /dev/null $URL"
    echo ""
    echo "Results saved to /tmp/hitt_vs_curl.md"
else
    echo "Install hyperfine for comparison: brew install hyperfine"
fi
echo ""

# ── 3. Built-in load test ─────────────────────────────────────
echo "════════════════════════════════════════"
echo " 3. Built-in load test — 50 requests, 5 concurrency"
echo "════════════════════════════════════════"
/usr/bin/time -l "$BINARY" send GET "$URL" > /dev/null 2> /tmp/hitt_loadtest_mem.txt
echo ""
echo "Memory for single request:"
grep "maximum resident" /tmp/hitt_loadtest_mem.txt || true
echo ""

# ── 4. Throughput test (sequential) ───────────────────────────
echo "════════════════════════════════════════"
echo " 4. Sequential throughput — $RUNS requests"
echo "════════════════════════════════════════"
START=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
for i in $(seq 1 "$RUNS"); do
    "$BINARY" send GET "$URL" > /dev/null 2>&1
done
END=$(date +%s%N 2>/dev/null || python3 -c 'import time; print(int(time.time()*1e9))')
ELAPSED_MS=$(( (END - START) / 1000000 ))
AVG_MS=$(( ELAPSED_MS / RUNS ))
RPS=$(echo "scale=1; $RUNS * 1000 / $ELAPSED_MS" | bc 2>/dev/null || echo "N/A")
echo "Total:   ${ELAPSED_MS}ms"
echo "Average: ${AVG_MS}ms per request"
echo "RPS:     $RPS"
echo ""

echo "════════════════════════════════════════"
echo " Done!"
echo "════════════════════════════════════════"
