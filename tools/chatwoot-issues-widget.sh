#!/usr/bin/env bash
# Before/after behaviour probes for the widget and label fixes. Prints one line per check.
# Usage: BASE=http://127.0.0.1:3000 PGDB=pulsar tools/chatwoot-issues-widget.sh
set -uo pipefail
B=${BASE:-http://127.0.0.1:3000}
DB=${PGDB:-pulsar}
WT=$(psql -d "$DB" -Atc "select website_token from channel_web_widgets order by id limit 1")
report() { printf '%-7s %s\n' "$1" "$2"; }
tok() { curl -s -X POST "$B/api/v1/widget/config?website_token=$WT" -H 'content-type: application/json' -d '{}' \
  | python3 -c 'import sys,json;print(json.load(sys.stdin)["website_channel_config"]["auth_token"])'; }
src_of() { python3 -c 'import sys,base64,json;p=sys.argv[1].split(".")[1];p+="="*(-len(p)%4);print(json.loads(base64.urlsafe_b64decode(p))["source_id"])' "$1"; }

# #16417: 5 concurrent first messages, 3 runs -> conversations per visitor
counts=""
for run in 1 2 3; do
  T=$(tok)
  for i in 1 2 3 4 5; do
    curl -s -o /dev/null -X POST "$B/api/v1/widget/messages?website_token=$WT" -H "X-Auth-Token: $T" \
      -H 'content-type: application/json' -d "{\"message\":{\"content\":\"race $i\"}}" &
  done
  wait
  S=$(src_of "$T")
  counts="$counts $(psql -d "$DB" -Atc "select count(*) from conversations c join contact_inboxes ci on ci.id=c.contact_inbox_id where ci.source_id='$S'")"
done
report 16417 "conversations per visitor after 5 concurrent first messages:$counts"

# #15779: HTML in a visitor message as stored
T=$(tok)
stored=$(curl -s -X POST "$B/api/v1/widget/messages?website_token=$WT" -H "X-Auth-Token: $T" -H 'content-type: application/json' \
  -d '{"message":{"content":"hi <img src=x onerror=alert(1)> **bold**"}}' | python3 -c 'import sys,json;print(repr(json.load(sys.stdin)["content"]))')
report 15779 "stored content: $stored"

# #15775: token in the /widget URL
hdr=$(curl -s -o /dev/null -D - "$B/widget?website_token=$WT&cw_conversation=$T" | tr -d '\r')
loc=$(echo "$hdr" | awk -F': ' 'tolower($1)=="location"{print $2}')
st=$(echo "$hdr" | head -1 | cut -d' ' -f2)
leak=$([ -n "$loc" ] && echo "$loc" | grep -q cw_conversation && echo yes || echo no)
rp=$(echo "$hdr" | awk -F': ' 'tolower($1)=="referrer-policy"{print $2}')
report 15775 "status=$st token_in_redirect=$leak referrer-policy=${rp:-none}"

# #15776 / #15933: contact update returns a fresh token and pubsub_token
resp=$(curl -s -X PATCH "$B/api/v1/widget/contact?website_token=$WT" -H "X-Auth-Token: $T" -H 'content-type: application/json' -d '{"name":"Probe"}')
report 15776 "widget_auth_token returned: $(echo "$resp" | python3 -c 'import sys,json;print(bool(json.load(sys.stdin).get("widget_auth_token")))' 2>/dev/null || echo error)"
report 15933 "pubsub_token returned: $(echo "$resp" | python3 -c 'import sys,json;print(bool(json.load(sys.stdin).get("pubsub_token")))' 2>/dev/null || echo error)"

# #15776: after rotation the earlier token is revoked (needs migrations/0003)
send() { curl -s -o /dev/null -w '%{http_code}' -X POST "$B/api/v1/widget/messages?website_token=$WT" -H "X-Auth-Token: $1" \
  -H 'content-type: application/json' -d '{"message":{"content":"probe"}}'; }
NEW=$(echo "$resp" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("widget_auth_token") or "")' 2>/dev/null)
report 15776 "old token send=$(send "$T") new token send=$([ -n "$NEW" ] && send "$NEW" || echo none)"
