#!/usr/bin/env bash
#
# GeointPlayback API test suite.
# Tests all endpoints and validates the full STAC → InSAR pipeline.
#
# Usage:
#   wash dev &     # start server first
#   ./tests/test_api.sh
#
set -euo pipefail

URL="${GEOINT_URL:-http://localhost:8000}"
PASSED=0
FAILED=0
ERRORS=""

pass() { PASSED=$((PASSED + 1)); printf "  %-50s  PASS\n" "$1"; }
fail() { FAILED=$((FAILED + 1)); ERRORS="$ERRORS\n  $1: $2"; printf "  %-50s  FAIL  (%s)\n" "$1" "$2"; }

# Check server
if ! curl -sf "$URL/" > /dev/null 2>&1; then
    echo "ERROR: Server not reachable at $URL"
    echo "Start it with: wash dev"
    exit 1
fi

echo "GeointPlayback API Tests"
echo "Server: $URL"
echo "========================"

# ── Test 1: GET / serves UI ──
echo ""
echo "[UI]"
STATUS=$(curl -sf -o /dev/null -w "%{http_code}" "$URL/")
CONTENT_TYPE=$(curl -sf -o /dev/null -w "%{content_type}" "$URL/")
if [[ "$STATUS" == "200" ]] && echo "$CONTENT_TYPE" | grep -q "text/html"; then
    pass "GET / serves HTML UI"
else
    fail "GET / serves HTML UI" "HTTP $STATUS, type=$CONTENT_TYPE"
fi

# ── Test 2: GET /api/sites returns validation sites ──
echo ""
echo "[Validation Sites]"
SITES=$(curl -sf "$URL/api/sites")
SITE_COUNT=$(echo "$SITES" | python3 -c "import sys,json; print(len(json.load(sys.stdin)))" 2>/dev/null || echo "0")
if [[ "$SITE_COUNT" -ge 3 ]]; then
    pass "GET /api/sites returns $SITE_COUNT sites"
else
    fail "GET /api/sites" "expected >= 3 sites, got $SITE_COUNT"
fi

# Validate site structure
HAS_BBOX=$(echo "$SITES" | python3 -c "import sys,json; d=json.load(sys.stdin); print('ok' if all('bbox' in s and len(s['bbox'])==4 for s in d) else 'fail')" 2>/dev/null || echo "fail")
if [[ "$HAS_BBOX" == "ok" ]]; then
    pass "Sites have valid bbox arrays"
else
    fail "Sites bbox structure" "missing or invalid"
fi

# ── Test 3: STAC Search ──
echo ""
echo "[STAC Search]"

# Test with LA Metro area
STAC_RESULT=$(curl -sf -X POST "$URL/api/stac/search" \
  -H "Content-Type: application/json" \
  -d '{"bbox":[-118.4,34.0,-118.2,34.1],"datetime":"2024-01-01/2024-06-30","limit":10}' 2>/dev/null || echo '{"features":[]}')

FEATURE_COUNT=$(echo "$STAC_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('features',[])))" 2>/dev/null || echo "0")
if [[ "$FEATURE_COUNT" -gt 0 ]]; then
    pass "STAC search returns $FEATURE_COUNT Sentinel-1 scenes"
else
    fail "STAC search (LA Metro)" "no features returned"
fi

# Verify features have expected properties
HAS_DATETIME=$(echo "$STAC_RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
ok = all('datetime' in f.get('properties',{}) for f in d.get('features',[]))
print('ok' if ok else 'fail')
" 2>/dev/null || echo "fail")
if [[ "$HAS_DATETIME" == "ok" ]]; then
    pass "STAC features have datetime properties"
else
    fail "STAC feature properties" "missing datetime"
fi

# Test with London area
LONDON_RESULT=$(curl -sf -X POST "$URL/api/stac/search" \
  -H "Content-Type: application/json" \
  -d '{"bbox":[-0.1,51.4,0.2,51.6],"datetime":"2024-01-01/2024-06-30","limit":5}' 2>/dev/null || echo '{"features":[]}')
LONDON_COUNT=$(echo "$LONDON_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('features',[])))" 2>/dev/null || echo "0")
if [[ "$LONDON_COUNT" -gt 0 ]]; then
    pass "STAC search London returns $LONDON_COUNT scenes"
else
    fail "STAC search London" "no features"
fi

# ── Test 4: InSAR Processing ──
echo ""
echo "[InSAR Processing]"

# Test with mock features
PROCESS_RESULT=$(curl -sf -X POST "$URL/api/process" \
  -H "Content-Type: application/json" \
  -d '{
    "bbox":[-118.35,34.055,-118.30,34.065],
    "datetime":"2020-01-01/2020-06-01",
    "features":[
      {"id":"s1","properties":{"datetime":"2020-01-15T00:00:00Z"}},
      {"id":"s2","properties":{"datetime":"2020-02-08T00:00:00Z"}},
      {"id":"s3","properties":{"datetime":"2020-03-03T00:00:00Z"}},
      {"id":"s4","properties":{"datetime":"2020-04-08T00:00:00Z"}},
      {"id":"s5","properties":{"datetime":"2020-05-02T00:00:00Z"}}
    ]
  }' 2>/dev/null || echo '{}')

FRAME_COUNT=$(echo "$PROCESS_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('frames',[])))" 2>/dev/null || echo "0")
if [[ "$FRAME_COUNT" -eq 5 ]]; then
    pass "InSAR returns $FRAME_COUNT frames (one per scene)"
else
    fail "InSAR frame count" "expected 5, got $FRAME_COUNT"
fi

# Validate displacement data structure
GRID_OK=$(echo "$PROCESS_RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
f = d['frames'][-1]
gw, gh = f['grid_w'], f['grid_h']
ok = len(f['displacement_mm']) == gw*gh and len(f['coherence']) == gw*gh
print('ok' if ok else 'fail')
" 2>/dev/null || echo "fail")
if [[ "$GRID_OK" == "ok" ]]; then
    pass "Displacement grid has correct dimensions"
else
    fail "Displacement grid structure" "size mismatch"
fi

# Verify subsidence is detected (negative values = subsidence)
HAS_SUBSIDENCE=$(echo "$PROCESS_RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
last = d['frames'][-1]['displacement_mm']
has_neg = any(v < -1.0 for v in last)
print('ok' if has_neg else 'fail')
" 2>/dev/null || echo "fail")
if [[ "$HAS_SUBSIDENCE" == "ok" ]]; then
    pass "Displacement includes subsidence (negative values)"
else
    fail "Subsidence detection" "no negative displacement"
fi

# Verify coherence is in valid range
COH_OK=$(echo "$PROCESS_RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
coh = d['frames'][-1]['coherence']
ok = all(0 <= c <= 1 for c in coh)
print('ok' if ok else 'fail')
" 2>/dev/null || echo "fail")
if [[ "$COH_OK" == "ok" ]]; then
    pass "Coherence values in [0, 1] range"
else
    fail "Coherence range" "out of bounds"
fi

# Verify temporal ordering
TEMPORAL_OK=$(echo "$PROCESS_RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
dates = [f['date'] for f in d['frames']]
print('ok' if dates == sorted(dates) else 'fail')
" 2>/dev/null || echo "fail")
if [[ "$TEMPORAL_OK" == "ok" ]]; then
    pass "Frames are temporally ordered"
else
    fail "Frame ordering" "not chronological"
fi

# Verify cumulative displacement grows over time
CUMULATIVE_OK=$(echo "$PROCESS_RESULT" | python3 -c "
import sys,json
d=json.load(sys.stdin)
# Check center pixel subsidence increases over time
center = d['grid_w'] * d['grid_h'] // 2 + d['grid_w'] // 2
vals = [f['displacement_mm'][center] for f in d['frames']]
# First frame should be 0, subsequent should trend negative
ok = vals[0] == 0 and vals[-1] < vals[0]
print('ok' if ok else 'fail')
" 2>/dev/null || echo "fail")
if [[ "$CUMULATIVE_OK" == "ok" ]]; then
    pass "Cumulative subsidence increases over time"
else
    fail "Cumulative trend" "center pixel not trending down"
fi

# ── Test 5: Error handling ──
echo ""
echo "[Error Handling]"

# Empty features
ERR_EMPTY=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$URL/api/process" \
  -H "Content-Type: application/json" \
  -d '{"bbox":[0,0,1,1],"datetime":"2020-01-01/2020-12-31","features":[]}')
if [[ "$ERR_EMPTY" == "502" ]]; then
    pass "Empty features returns 502"
else
    fail "Empty features error" "expected 502, got $ERR_EMPTY"
fi

# 404 on unknown path
ERR_404=$(curl -s -o /dev/null -w "%{http_code}" "$URL/api/nonexistent")
if [[ "$ERR_404" == "404" ]]; then
    pass "Unknown path returns 404"
else
    fail "404 handling" "expected 404, got $ERR_404"
fi

# ── Test 6: End-to-end with real STAC data ──
echo ""
echo "[End-to-End Pipeline]"

if [[ "$FEATURE_COUNT" -gt 1 ]]; then
    E2E_BODY=$(echo "$STAC_RESULT" | python3 -c "
import sys, json
stac = json.load(sys.stdin)
features = [{'id': f['id'], 'properties': {'datetime': f['properties'].get('datetime','')}} for f in stac['features'][:10]]
print(json.dumps({'bbox': [-118.35, 34.05, -118.30, 34.07], 'datetime': '2024-01-01/2024-06-30', 'features': features}))
" 2>/dev/null)

    E2E_RESULT=$(curl -sf -X POST "$URL/api/process" \
      -H "Content-Type: application/json" \
      -d "$E2E_BODY" 2>/dev/null || echo '{}')

    E2E_FRAMES=$(echo "$E2E_RESULT" | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('frames',[])))" 2>/dev/null || echo "0")
    if [[ "$E2E_FRAMES" -gt 1 ]]; then
        MAX_SUB=$(echo "$E2E_RESULT" | python3 -c "import sys,json; print(round(json.load(sys.stdin)['max_subsidence_mm'],1))" 2>/dev/null)
        pass "E2E: STAC → InSAR produces $E2E_FRAMES frames (max sub: ${MAX_SUB}mm)"
    else
        fail "E2E pipeline" "expected frames, got $E2E_FRAMES"
    fi
else
    echo "  (skipped — STAC returned insufficient data)"
fi

# ── Summary ──
echo ""
echo "================================================================"
TOTAL=$((PASSED + FAILED))
echo "Results: $PASSED/$TOTAL passed, $FAILED failed"

if [[ $FAILED -gt 0 ]]; then
    echo ""
    echo "Failures:"
    echo -e "$ERRORS"
fi

exit $FAILED
