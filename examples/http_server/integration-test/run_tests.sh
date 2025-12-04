#!/bin/bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

SERVER_URL="${SERVER_URL:-http://pulsive:8080}"
BACKEND1_URL="${BACKEND1_URL:-http://backend1:8000}"
BACKEND2_URL="${BACKEND2_URL:-http://backend2:8000}"

echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}  Pulsive HTTP Server Integration Test${NC}"
echo -e "${BLUE}======================================${NC}"
echo ""

# Wait for services to be ready
wait_for_service() {
    local url=$1
    local name=$2
    local max_attempts=30
    local attempt=1
    
    echo -n "Waiting for $name..."
    while [ $attempt -le $max_attempts ]; do
        if curl -sf "$url" > /dev/null 2>&1; then
            echo -e " ${GREEN}ready${NC}"
            return 0
        fi
        echo -n "."
        sleep 1
        attempt=$((attempt + 1))
    done
    echo -e " ${RED}failed${NC}"
    return 1
}

echo -e "${YELLOW}[Phase 1] Waiting for services to be ready...${NC}"
wait_for_service "$BACKEND1_URL/health" "Backend 1"
wait_for_service "$BACKEND2_URL/health" "Backend 2"
wait_for_service "$SERVER_URL/" "Pulsive Server"
echo ""

# Give health checks time to detect backends
echo "Waiting for health checks to complete..."
sleep 10

echo -e "${YELLOW}[Phase 2] Functional Tests${NC}"
echo "-----------------------------------"

# Test 1: Static file serving
echo -n "Test 1: Static file serving... "
response=$(curl -sf "$SERVER_URL/" | head -c 100)
if echo "$response" | grep -q "DOCTYPE"; then
    echo -e "${GREEN}PASS${NC}"
else
    echo -e "${RED}FAIL${NC}"
fi

# Test 2: Directory listing
echo -n "Test 2: Directory listing... "
response=$(curl -sf "$SERVER_URL/static/")
if echo "$response" | grep -q "Index of"; then
    echo -e "${GREEN}PASS${NC}"
else
    echo -e "${RED}FAIL${NC}"
fi

# Test 3: Load balancing - check requests go to different backends
echo -n "Test 3: Load balancing distribution... "
server1_count=0
server2_count=0
for i in $(seq 1 20); do
    response=$(curl -sf "$SERVER_URL/api/echo" 2>/dev/null || echo '{"server_id":"error"}')
    server_id=$(echo "$response" | jq -r '.server_id' 2>/dev/null || echo "error")
    if [ "$server_id" = "backend-1" ]; then
        server1_count=$((server1_count + 1))
    elif [ "$server_id" = "backend-2" ]; then
        server2_count=$((server2_count + 1))
    fi
done
if [ $server1_count -gt 0 ] && [ $server2_count -gt 0 ]; then
    echo -e "${GREEN}PASS${NC} (backend-1: $server1_count, backend-2: $server2_count)"
else
    echo -e "${YELLOW}PARTIAL${NC} (backend-1: $server1_count, backend-2: $server2_count)"
fi

# Test 4: Redirect
echo -n "Test 4: HTTP redirect... "
status=$(curl -sI "$SERVER_URL/old-page" | head -1 | awk '{print $2}')
if [ "$status" = "301" ]; then
    echo -e "${GREEN}PASS${NC}"
else
    echo -e "${RED}FAIL${NC} (got status: $status)"
fi

echo ""
echo -e "${YELLOW}[Phase 3] Performance Tests${NC}"
echo "-----------------------------------"

# Performance test: Static files
echo ""
echo -e "${BLUE}Test: Static file performance (1000 requests, 50 concurrent)${NC}"
hey -n 1000 -c 50 "$SERVER_URL/" 2>/dev/null | grep -E "(Requests/sec|Average|Fastest|Slowest|Status code)"

# Performance test: API with load balancing
echo ""
echo -e "${BLUE}Test: Load balanced API (1000 requests, 50 concurrent)${NC}"
hey -n 1000 -c 50 "$SERVER_URL/api/echo" 2>/dev/null | grep -E "(Requests/sec|Average|Fastest|Slowest|Status code)"

# Performance test: High concurrency
echo ""
echo -e "${BLUE}Test: High concurrency (5000 requests, 200 concurrent)${NC}"
hey -n 5000 -c 200 "$SERVER_URL/" 2>/dev/null | grep -E "(Requests/sec|Average|Fastest|Slowest|Status code)"

echo ""
echo -e "${YELLOW}[Phase 4] Rate Limiting Test${NC}"
echo "-----------------------------------"
echo -n "Test: Rate limiting on /api... "
# Make 150 rapid requests (limit is 100/minute)
limited_count=0
for i in $(seq 1 150); do
    status=$(curl -so /dev/null -w "%{http_code}" "$SERVER_URL/api/echo" 2>/dev/null)
    if [ "$status" = "429" ]; then
        limited_count=$((limited_count + 1))
    fi
done
if [ $limited_count -gt 0 ]; then
    echo -e "${GREEN}PASS${NC} ($limited_count requests rate-limited)"
else
    echo -e "${YELLOW}PARTIAL${NC} (no requests were rate-limited)"
fi

echo ""
echo -e "${BLUE}======================================${NC}"
echo -e "${BLUE}  Integration Tests Complete!${NC}"
echo -e "${BLUE}======================================${NC}"

