#!/bin/bash
set -e

URL="${1:-https://httpbin.org/get}"
RUNS="${2:-20}"
LOAD_N="${3:-200}"
LOAD_C="${4:-10}"

HITT="target/release/hitt"

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║              hitt — Performance Comparison                  ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  URL:          $URL"
echo "  Runs:         $RUNS"
echo "  Load test:    $LOAD_N requests @ $LOAD_C concurrency"
echo ""

# Build release if needed
if [ ! -f "$HITT" ]; then
    echo "Building release binary..."
    cargo build --release --quiet
fi

# ═══════════════════════════════════════════════════════════════════
# PART 1: Single request speed (hitt vs curl vs xh vs httpie vs wget)
# ═══════════════════════════════════════════════════════════════════
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " 1. SINGLE REQUEST SPEED — who is fastest?"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

CMDS=()
CMDS+=("$HITT send GET $URL")
CMDS+=("curl -s -o /dev/null -w '' $URL")

if command -v xh &>/dev/null; then
    CMDS+=("xh GET $URL -q")
fi
if command -v http &>/dev/null; then
    CMDS+=("http --print= GET $URL")
fi
if command -v wget &>/dev/null; then
    CMDS+=("wget -q -O /dev/null $URL")
fi

hyperfine \
    --warmup 3 \
    --runs "$RUNS" \
    --export-markdown /tmp/hitt_compare_speed.md \
    "${CMDS[@]}"

echo ""
echo "  Full table saved to: /tmp/hitt_compare_speed.md"
echo ""

# ═══════════════════════════════════════════════════════════════════
# PART 2: Memory usage comparison
# ═══════════════════════════════════════════════════════════════════
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " 2. MEMORY USAGE — peak RSS per single request"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

printf "  %-12s %s\n" "TOOL" "PEAK MEMORY"
printf "  %-12s %s\n" "────" "───────────"

for tool_cmd in \
    "hitt:$HITT send GET $URL" \
    "curl:curl -s -o /dev/null $URL" \
    "xh:xh GET $URL -q" \
    "httpie:http --print= GET $URL" \
    "wget:wget -q -O /dev/null $URL"
do
    name="${tool_cmd%%:*}"
    cmd="${tool_cmd#*:}"
    bin="${cmd%% *}"

    if ! command -v "$bin" &>/dev/null && [ ! -f "$bin" ]; then
        continue
    fi

    mem_bytes=$(/usr/bin/time -l sh -c "$cmd" 2>&1 >/dev/null | grep "maximum resident" | awk '{print $1}')
    if [ -n "$mem_bytes" ]; then
        mem_mb=$(echo "scale=1; $mem_bytes / 1048576" | bc)
        printf "  %-12s %s MB  (%s bytes)\n" "$name" "$mem_mb" "$mem_bytes"
    fi
done
echo ""

# ═══════════════════════════════════════════════════════════════════
# PART 3: Startup time (no network)
# ═══════════════════════════════════════════════════════════════════
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " 3. STARTUP TIME — --help (no network, pure binary overhead)"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

HELP_CMDS=()
HELP_CMDS+=("$HITT --help")
HELP_CMDS+=("curl --help")
if command -v xh &>/dev/null; then HELP_CMDS+=("xh --help"); fi
if command -v http &>/dev/null; then HELP_CMDS+=("http --help"); fi

hyperfine \
    --warmup 5 \
    --runs 50 \
    --export-markdown /tmp/hitt_compare_startup.md \
    "${HELP_CMDS[@]}"

echo ""
echo "  Full table saved to: /tmp/hitt_compare_startup.md"
echo ""

# ═══════════════════════════════════════════════════════════════════
# PART 4: Load test / throughput comparison
# ═══════════════════════════════════════════════════════════════════
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " 4. LOAD TEST — $LOAD_N requests @ $LOAD_C concurrency"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# hey
if command -v hey &>/dev/null; then
    echo "  ── hey ──"
    hey -n "$LOAD_N" -c "$LOAD_C" -q 0 "$URL" 2>&1 | grep -E "Total:|Requests/sec:|Average:|Fastest:|Slowest:|Status code"
    echo ""
fi

# wrk (duration-based, approximate)
if command -v wrk &>/dev/null; then
    echo "  ── wrk (5s burst) ──"
    wrk -t2 -c"$LOAD_C" -d5s "$URL" 2>&1
    echo ""
fi

# ab
if command -v ab &>/dev/null; then
    echo "  ── ab (Apache Bench) ──"
    ab -n "$LOAD_N" -c "$LOAD_C" -q "$URL" 2>&1 | grep -E "Requests per second|Time per request|Transfer rate|Complete|Failed"
    echo ""
fi

# ═══════════════════════════════════════════════════════════════════
# PART 5: Binary size comparison
# ═══════════════════════════════════════════════════════════════════
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo " 5. BINARY SIZE"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
printf "  %-12s %s\n" "TOOL" "SIZE"
printf "  %-12s %s\n" "────" "────"

for tool in "$HITT" "$(which curl)" "$(which xh 2>/dev/null)" "$(which http 2>/dev/null)" "$(which wget 2>/dev/null)"; do
    if [ -n "$tool" ] && [ -f "$tool" ]; then
        name=$(basename "$tool")
        size=$(du -h "$tool" | cut -f1)
        printf "  %-12s %s\n" "$name" "$size"
    fi
done
echo ""

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║  Done! Results in /tmp/hitt_compare_*.md                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
