#!/usr/bin/env bash
set -euo pipefail

API_BASE_URL="${AGENTICS_API_BASE_URL:-http://127.0.0.1:3000}"
WEB_BASE_URL="${AGENTICS_WEB_BASE_URL:-}"
ADMIN_USERNAME="${AGENTICS_ADMIN_USERNAME:-admin}"
ADMIN_PASSWORD="${AGENTICS_ADMIN_PASSWORD:-}"

info() {
  printf '[agentics-check] %s\n' "$*"
}

fail() {
  printf '[agentics-check] ERROR: %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

http_get() {
  curl -fsS "$1"
}

http_get_admin() {
  curl -fsS -u "${ADMIN_USERNAME}:${ADMIN_PASSWORD}" "$1"
}

require_command curl
require_command docker
require_command python3

info "checking Docker daemon"
docker info >/dev/null

info "checking API health at ${API_BASE_URL}/healthz"
health_json="$(http_get "${API_BASE_URL}/healthz")"
printf '%s' "$health_json" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)
if payload.get("status") != "ok":
    raise SystemExit("health status is not ok")
database = payload.get("database", {})
if database.get("connected") is not True:
    raise SystemExit("database is not connected")
'

info "checking public challenge catalog"
challenges_json="$(http_get "${API_BASE_URL}/api/public/challenges")"
printf '%s' "$challenges_json" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)
items = payload.get("items")
if not isinstance(items, list):
    raise SystemExit("challenge catalog did not return an items array")
print(f"[agentics-check] public challenges: {len(items)}")
'

if [[ -n "$ADMIN_PASSWORD" ]]; then
  info "checking admin capacity"
  capacity_json="$(http_get_admin "${API_BASE_URL}/admin/capacity")"
  printf '%s' "$capacity_json" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)
if "quotas" not in payload or "usage" not in payload:
    raise SystemExit("admin capacity response is missing quotas or usage")
'

  info "checking worker heartbeat list"
  heartbeats_json="$(http_get_admin "${API_BASE_URL}/admin/service-heartbeats")"
  printf '%s' "$heartbeats_json" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)
items = payload.get("items")
if not isinstance(items, list):
    raise SystemExit("heartbeat response did not return an items array")
print(f"[agentics-check] service heartbeats: {len(items)}")
'
else
  info "skipping admin checks because AGENTICS_ADMIN_PASSWORD is unset"
fi

if [[ -n "$WEB_BASE_URL" ]]; then
  info "checking web frontend at ${WEB_BASE_URL}"
  http_get "$WEB_BASE_URL" >/dev/null
else
  info "skipping web check because AGENTICS_WEB_BASE_URL is unset"
fi

info "local MVP checks passed"
