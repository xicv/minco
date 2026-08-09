#!/bin/sh
set -eu

script_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
repository_root=$(CDPATH='' cd -- "$script_dir/../../.." && pwd)
mailpit_url=${MINCO_MAILPIT_HTTP_URL:-http://127.0.0.1:${MINCO_MAILPIT_UI_PORT:-8025}}
temporary_dir=$(mktemp -d)
message_json="$temporary_dir/message.json"
raw_message="$temporary_dir/message.eml"
attachment_file="$temporary_dir/attachment.bin"
inline_file="$temporary_dir/inline.bin"
cleanup() {
    rm -f "$message_json" "$raw_message" "$attachment_file" "$inline_file"
    rmdir "$temporary_dir"
}
trap cleanup EXIT HUP INT TERM

"$script_dir/mailpit-ready.sh"
before_count=$(curl --fail --silent --show-error --max-time 2 \
    "$mailpit_url/api/v1/info" | jq --raw-output '.Messages')
cd "$repository_root"
cargo run --locked -p minco-plugin-notifications --example mailpit_smoke

attempt=1
while [ "$attempt" -le 10 ]; do
    if curl --fail --silent --show-error --max-time 2 \
        "$mailpit_url/api/v1/message/latest" >"$message_json"; then
        break
    fi
    attempt=$((attempt + 1))
    sleep 1
done

jq --exit-status '
    .Subject == "Minco Mailpit smoke ✓"
    and .Text == "Minco Mailpit plain-text smoke body"
    and (.HTML | contains("Minco <strong>Mailpit</strong> HTML smoke body"))
    and (.To | map(.Address) | index("person@example.test") != null)
    and (.Cc | map(.Address) | index("accounts@example.test") != null)
    and (.ReplyTo | map(.Address) | index("support@example.test") != null)
    and (.Bcc | map(.Address) | index("audit@example.test") != null)
    and (.Attachments | length == 1)
    and .Attachments[0].FileName == "evidence.txt"
    and .Attachments[0].ContentType == "text/plain"
    and .Attachments[0].Checksums.SHA256 == "0011c0c24a084d056f4db85e98ae94c6f24cf4241807d92d5cf3003e05a4817d"
    and (.Inline | length == 1)
    and .Inline[0].FileName == "logo.svg"
    and .Inline[0].ContentType == "image/svg+xml"
    and .Inline[0].ContentID == "logo"
    and .Inline[0].Checksums.SHA256 == "900fbe934249ad120004bd24adf66aad8817d89586273c0cc50e187bddebb601"
' "$message_json" >/dev/null

after_count=$(curl --fail --silent --show-error --max-time 2 \
    "$mailpit_url/api/v1/info" | jq --raw-output '.Messages')
test "$after_count" -eq $((before_count + 1))

message_id=$(jq --raw-output '.ID' "$message_json")
curl --fail --silent --show-error --max-time 2 \
    "$mailpit_url/api/v1/message/$message_id/raw" >"$raw_message"
# Mailpit reconstructs Bcc from SMTP envelope metadata in this API response;
# the byte-exact SMTP unit test separately proves Minco omits Bcc from its MIME.
grep -Fq 'X-Minco-Smoke: mailpit-local' "$raw_message"

attachment_part=$(jq --raw-output '.Attachments[0].PartID' "$message_json")
inline_part=$(jq --raw-output '.Inline[0].PartID' "$message_json")
curl --fail --silent --show-error --max-time 2 \
    "$mailpit_url/api/v1/message/$message_id/part/$attachment_part" >"$attachment_file"
curl --fail --silent --show-error --max-time 2 \
    "$mailpit_url/api/v1/message/$message_id/part/$inline_part" >"$inline_file"
test "$(shasum -a 256 "$attachment_file" | cut -d' ' -f1)" = \
    "0011c0c24a084d056f4db85e98ae94c6f24cf4241807d92d5cf3003e05a4817d"
test "$(shasum -a 256 "$inline_file" | cut -d' ' -f1)" = \
    "900fbe934249ad120004bd24adf66aad8817d89586273c0cc50e187bddebb601"

printf '%s\n' "Mailpit SMTP/API rich-message smoke passed"
