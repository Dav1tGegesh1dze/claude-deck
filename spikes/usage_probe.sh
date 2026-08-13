#!/bin/bash
# Phase 0 data spike: confirms we can read Claude Code's cached OAuth token
# and call the (undocumented) usage endpoint with it. macOS only for now —
# reads the token from Keychain. Never prints the raw token.
set -euo pipefail

RAW=$(security find-generic-password -a "$(whoami)" -s "Claude Code-credentials" -w)

TOKEN=$(python3 -c "
import json, sys
data = json.loads(sys.argv[1])
def find_token(o):
    if isinstance(o, dict):
        for k, v in o.items():
            if isinstance(v, str) and k.lower() in ('accesstoken', 'access_token'):
                return v
            found = find_token(v)
            if found:
                return found
    elif isinstance(o, list):
        for v in o:
            found = find_token(v)
            if found:
                return found
    return None
print(find_token(data) or '')
" "$RAW")

if [ -z "$TOKEN" ]; then
  echo "No access token found in Keychain credential blob." >&2
  exit 1
fi

curl -s \
  -H "Authorization: Bearer $TOKEN" \
  -H "anthropic-beta: oauth-2025-04-20" \
  "https://api.anthropic.com/api/oauth/usage" | python3 -m json.tool
