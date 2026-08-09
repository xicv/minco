#!/bin/sh
set -eu

mailpit_url=${MINCO_MAILPIT_HTTP_URL:-http://127.0.0.1:${MINCO_MAILPIT_UI_PORT:-8025}}
attempt=1
while [ "$attempt" -le 30 ]; do
    if curl --fail --silent --show-error --max-time 2 "$mailpit_url/readyz" >/dev/null; then
        printf '%s\n' "Mailpit is ready at $mailpit_url"
        exit 0
    fi
    attempt=$((attempt + 1))
    sleep 1
done

printf '%s\n' "Mailpit did not become ready at $mailpit_url within 30 seconds" >&2
exit 1
