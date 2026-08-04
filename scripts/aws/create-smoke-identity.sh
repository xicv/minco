#!/usr/bin/env bash
set -euo pipefail

# shellcheck source=scripts/aws/lib/common.sh
source "$(dirname "$0")/lib/common.sh"
repo_root="$(minco_repo_root)"
cd "$repo_root"

for command in aws jq openssl; do
  require_command "$command"
done
: "${AWS_REGION:=ap-southeast-2}"
initialize_cloud_journal

pool_name="minco-smoke-$MINCO_AWS_RUN_ID"
username="minco-smoke"
request_directory="$(mktemp -d /tmp/minco-cognito.XXXXXX)"
chmod 700 "$request_directory"
cleanup_requests() {
  rm -f \
    "$request_directory/create-pool.json" \
    "$request_directory/create-user.json" \
    "$request_directory/set-password.json" \
    "$request_directory/auth.json"
  rmdir "$request_directory" >/dev/null 2>&1 || true
}
trap cleanup_requests EXIT

jq -n \
  --arg pool_name "$pool_name" \
  --arg run_id "$MINCO_AWS_RUN_ID" \
  '{
    PoolName: $pool_name,
    DeletionProtection: "INACTIVE",
    UserPoolTier: "LITE",
    UserPoolTags: {
      "minco:managed": "true",
      "minco:purpose": "bounded-smoke",
      "minco:run-id": $run_id
    },
    Policies: {
      PasswordPolicy: {
        MinimumLength: 16,
        RequireUppercase: true,
        RequireLowercase: true,
        RequireNumbers: true,
        RequireSymbols: true,
        TemporaryPasswordValidityDays: 1
      }
    },
    Schema: [{
      Name: "permissions",
      AttributeDataType: "String",
      DeveloperOnlyAttribute: false,
      Mutable: false,
      Required: false,
      StringAttributeConstraints: {MinLength: "1", MaxLength: "256"}
    }]
  }' >"$request_directory/create-pool.json"
chmod 600 "$request_directory/create-pool.json"
pool_id="$(
  aws_logged cognito-idp create-user-pool \
    "create temporary Lite user pool $pool_name with immutable permission attribute" \
    --cli-input-json "file://$request_directory/create-pool.json" \
    --query UserPool.Id \
    --output text
)"
[[ -n "$pool_id" && "$pool_id" != "None" ]]
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/cognito-pool-id.txt" "$pool_id"

client_id="$(
  aws_logged cognito-idp create-user-pool-client \
    "create temporary non-secret smoke client in $pool_id" \
    --user-pool-id "$pool_id" \
    --client-name "minco-smoke" \
    --no-generate-secret \
    --explicit-auth-flows ALLOW_ADMIN_USER_PASSWORD_AUTH \
    --prevent-user-existence-errors ENABLED \
    --access-token-validity 60 \
    --id-token-validity 60 \
    --token-validity-units AccessToken=minutes,IdToken=minutes \
    --read-attributes custom:permissions \
    --query UserPoolClient.ClientId \
    --output text
)"
[[ -n "$client_id" && "$client_id" != "None" ]]
write_evidence_value "$MINCO_AWS_EVIDENCE_DIR/cognito-client-id.txt" "$client_id"

password="Aa1!$(openssl rand -hex 24)"
jq -n \
  --arg pool "$pool_id" \
  --arg username "$username" \
  --arg password "$password" \
  '{
    UserPoolId: $pool,
    Username: $username,
    TemporaryPassword: $password,
    MessageAction: "SUPPRESS",
    UserAttributes: [
      {Name: "custom:permissions", Value: "orders.create orders.read"}
    ]
  }' >"$request_directory/create-user.json"
chmod 600 "$request_directory/create-user.json"
aws_logged cognito-idp admin-create-user \
  "create temporary synthetic principal in $pool_id; password redacted" \
  --cli-input-json "file://$request_directory/create-user.json" >/dev/null

jq -n \
  --arg pool "$pool_id" \
  --arg username "$username" \
  --arg password "$password" \
  '{
    UserPoolId: $pool,
    Username: $username,
    Password: $password,
    Permanent: true
  }' >"$request_directory/set-password.json"
chmod 600 "$request_directory/set-password.json"
aws_logged cognito-idp admin-set-user-password \
  "make temporary synthetic principal usable; password redacted" \
  --cli-input-json "file://$request_directory/set-password.json" >/dev/null

jq -n \
  --arg pool "$pool_id" \
  --arg client "$client_id" \
  --arg username "$username" \
  --arg password "$password" \
  '{
    UserPoolId: $pool,
    ClientId: $client,
    AuthFlow: "ADMIN_USER_PASSWORD_AUTH",
    AuthParameters: {
      USERNAME: $username,
      PASSWORD: $password
    }
  }' >"$request_directory/auth.json"
chmod 600 "$request_directory/auth.json"
token="$(
  aws_logged cognito-idp admin-initiate-auth \
    "issue 60-minute ID token for synthetic smoke principal; token redacted" \
    --cli-input-json "file://$request_directory/auth.json" \
    --query AuthenticationResult.IdToken \
    --output text
)"
[[ -n "$token" && "$token" != "None" ]]
unset password

write_evidence_value \
  "$MINCO_AWS_EVIDENCE_DIR/jwt-issuer.txt" \
  "https://cognito-idp.$AWS_REGION.amazonaws.com/$pool_id"
printf '%s\n' "$token"
