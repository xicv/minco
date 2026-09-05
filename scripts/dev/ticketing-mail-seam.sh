#!/usr/bin/env bash
set -euo pipefail
# Local Rustack seam proof for the ticketing inbound mail chain
# (Stage D2 slice 3b part 2 / ADR-0064).
#
# Proves, against live local S3+SQS services only (no AWS provider):
#   1. a real S3 `Records` envelope delivered through real SQS is parsed
#      by the worker wake handler (urlDecodedKey-aware);
#   2. the raw MIME is fetched from the real S3 bucket through the
#      S3 object-storage adapter;
#   3. threading resolves against the seeded anchor and exactly one
#      durable `ticketing.process-inbound-email` job lands in SQLite;
#   4. redelivery of the same envelope dedupes (still exactly one job).
#
# Rustack 0.9.1 exposes s3,sqs,ssm,sts; SES availability is probed and
# recorded, never assumed — the SES receiving binding stays plan-level.
cd "$(dirname "$0")/../.."

for command in aws cargo docker python3 sqlite3; do
  command -v "$command" >/dev/null || {
    printf '%s is required for the ticketing mail seam proof\n' "$command" >&2
    exit 1
  }
done

seam_port="$(
  python3 - <<'PY'
import socket
with socket.socket() as listener:
    listener.bind(("127.0.0.1", 0))
    print(listener.getsockname()[1])
PY
)"
seam_suffix="$(date -u +%Y%m%d%H%M%S)-$$"
seam_directory="$(mktemp -d /tmp/minco-ticketing-seam.XXXXXX)"
db_path="$seam_directory/ticketing.sqlite"
mime_path="$seam_directory/reply.eml"
bucket_name="minco-ticketing-seam-$seam_suffix"
queue_name="minco-ticketing-seam-$seam_suffix"
queue_url=""
ses_probe="unsupported"

export COMPOSE_PROJECT_NAME="minco-ticketing-seam-$$"
export MINCO_RUSTACK_PORT="$seam_port"
export MINCO_RUSTACK_SERVICES="s3,sqs,ssm,sts"
export AWS_ACCESS_KEY_ID="test"
export AWS_SECRET_ACCESS_KEY="test"
export AWS_DEFAULT_REGION="ap-southeast-2"
export AWS_EC2_METADATA_DISABLED="true"
export AWS_ENDPOINT_URL="http://127.0.0.1:$seam_port"
export AWS_MAX_ATTEMPTS="1"
export SEAM_DB="$db_path"
export SEAM_BUCKET="$bucket_name"
export SEAM_MAILBOX_SCOPE="support@example.test"

aws_local() {
  aws --cli-connect-timeout 1 --cli-read-timeout 5 "$@"
}

cleanup() {
  set +e
  if [[ -n "$queue_url" ]]; then
    aws_local sqs delete-queue --queue-url "$queue_url" >/dev/null 2>&1
  fi
  aws_local s3 rm "s3://$bucket_name" --recursive >/dev/null 2>&1
  aws_local s3api delete-bucket --bucket "$bucket_name" >/dev/null 2>&1
  docker compose -f infra/local/compose.yaml down --remove-orphans >/dev/null 2>&1
  rm -f "$mime_path"
  rm -rf "$seam_directory"
}
trap cleanup EXIT

docker compose -f infra/local/compose.yaml up -d rustack >/dev/null

ready=false
for _attempt in {1..40}; do
  if aws_local s3api list-buckets >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 1
done
[[ "$ready" == true ]] || { echo "rustack did not become ready" >&2; exit 1; }

# Honest capability probe: is SES implemented by this Rustack build?
if aws_local ses list-identities >/dev/null 2>&1; then
  ses_probe="available"
fi
printf 'ses probe: %s (recorded; SES receiving binding stays plan-level)\n' "$ses_probe"

aws_local s3api create-bucket --bucket "$bucket_name" >/dev/null
queue_url="$(
  aws_local sqs create-queue --queue-name "$queue_name" \
    --attributes '{"VisibilityTimeout": "10"}' --query 'QueueUrl' --output text
)"
export SEAM_QUEUE_URL="$queue_url"

printf 'From: user-1@example.test\r\nTo: support@example.test\r\nMessage-ID: <reply-w@example.test>\r\nIn-Reply-To: <original-1@example.test>\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nA seam-delivered reply.\r\n' > "$mime_path"

# The object lands at the decoded key; the notification carries the
# percent-encoded key plus urlDecodedKey, as real S3 delivers them.
object_key="mail/project-a/reply-1"
# Foreign producer: SES's receiving rule writes raw MIME with no Minco
# metadata and no content type; the read path must tolerate exactly that.
aws_local s3api put-object --bucket "$bucket_name" --key "$object_key" \
  --body "$mime_path" >/dev/null

envelope="$(
  event_time="$(date -u +%Y-%m-%dT%H:%M:%S.000Z)"
  python3 - "$event_time" <<'PY'
import json, sys
record = {
    "eventVersion": "2.1",
    "eventSource": "aws:s3",
    "awsRegion": "ap-southeast-2",
    "eventTime": sys.argv[1],
    "eventName": "ObjectCreated:Put",
    "userIdentity": {"principalId": "AWS:AROEXAMPLE:seam"},
    "requestParameters": {"sourceIPAddress": "127.0.0.1"},
    "responseElements": {
        "x-amz-request-id": "SEAMREQUEST",
        "x-amz-id-2": "seam/host",
    },
    "s3": {
        "s3SchemaVersion": "1.0",
        "configurationId": "ticketing-inbound",
        "bucket": {
            "name": __import__("os").environ["SEAM_BUCKET"],
            "ownerIdentity": {"principalId": "ASEAM"},
            "arn": "arn:aws:s3:::seam",
        },
        "object": {
            "key": "mail/project-a/reply%2D1",
            "size": 200,
            "eTag": "0123456789abcdef0123456789abcdef",
            "sequencer": "0051A4F9D53640D5",
            "urlDecodedKey": "mail/project-a/reply-1",
        },
    },
}
print(json.dumps({"Records": [record]}))
PY
)"

cargo run -q --locked -p minco-aws-worker --example ticketing_mail_seam \
  --features ticketing-wake -- seed

# Two deliveries of the same envelope: at-least-once in, dedupe out.
aws_local sqs send-message --queue-url "$queue_url" --message-body "$envelope" >/dev/null
aws_local sqs send-message --queue-url "$queue_url" --message-body "$envelope" >/dev/null

cargo run -q --locked -p minco-aws-worker --example ticketing_mail_seam \
  --features ticketing-wake -- poll
cargo run -q --locked -p minco-aws-worker --example ticketing_mail_seam \
  --features ticketing-wake -- poll >/dev/null
cargo run -q --locked -p minco-aws-worker --example ticketing_mail_seam \
  --features ticketing-wake -- verify

printf 'ticketing mail seam proof passed (ses: %s)\n' "$ses_probe"
