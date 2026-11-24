#!/usr/bin/env bash
# deployment/run_deployment.sh
# Provision the production infrastructure (EC2 + private RDS) with OpenTofu, then deploy
# the images that builds/publish_images.sh pushed to Docker Hub, using Docker Compose.
#
# Usage:  ./run_deployment.sh [--yes]
#   --yes   apply the plan without asking
# Config: deployment/.env.production (copy .env.production.example). Override its path with ENV_FILE.
# Safe to re-run: it applies infrastructure changes, refreshes the config, pulls images, and
# recreates only the containers whose image or configuration changed.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
cd -- "$SCRIPT_DIR"

die() {
  echo "error: $*" >&2
  exit 1
}

AUTO_APPROVE=0
while (($#)); do
  case "$1" in
    --yes) AUTO_APPROVE=1 ;;
    -h | --help)
      sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
  shift
done

# --- 0. Load and validate configuration --------------------------------------
ENV_FILE="${ENV_FILE:-$SCRIPT_DIR/.env.production}"
[[ -f "$ENV_FILE" ]] || die "$ENV_FILE not found. Copy .env.production.example and fill it in."

# Parse KEY=value lines instead of `source`-ing the file, so it is never executed as shell code.
# Only the keys below are accepted; "$HOME" and a leading "~/" are expanded in values.
allowed_keys=" DOCKERHUB_USER DOCKERHUB_TOKEN IMAGE_TAG JWT_SECRET REDIS_PASSWORD DB_USERNAME DB_PASSWORD AWS_REGION SSH_CIDR SSH_KEY_PATH TF_BIN "
line_no=0
re_line='^[[:space:]]*([A-Za-z_][A-Za-z0-9_]*)=(.*)$'
while IFS= read -r line || [[ -n "$line" ]]; do
  line_no=$((line_no + 1))
  line="${line%$'\r'}"
  [[ -z "${line//[[:space:]]/}" || "$line" =~ ^[[:space:]]*# ]] && continue
  # Do not echo the line itself: it may contain a secret.
  [[ "$line" =~ $re_line ]] || die "$ENV_FILE line $line_no is not KEY=value"
  key="${BASH_REMATCH[1]}"
  value="${BASH_REMATCH[2]}"
  if [[ "$value" =~ ^\"(.*)\"$ || "$value" =~ ^\'(.*)\'$ ]]; then
    value="${BASH_REMATCH[1]}"
  fi
  value="${value//\$HOME/$HOME}"
  # shellcheck disable=SC2088 # the literal two characters "~/" are what we match
  [[ "$value" == "~/"* ]] && value="$HOME/${value#\~/}"
  if [[ "$allowed_keys" == *" $key "* ]]; then
    export "$key=$value"
  else
    echo "warning: ignoring unknown key $key in $ENV_FILE (line $line_no)" >&2
  fi
done <"$ENV_FILE"

for name in DOCKERHUB_USER JWT_SECRET REDIS_PASSWORD DB_USERNAME DB_PASSWORD; do
  [[ -n "${!name:-}" ]] || die "$name is empty in $ENV_FILE"
done

IMAGE_TAG="${IMAGE_TAG:-latest}"
re_user='^[a-z0-9]{4,30}$'
re_tag='^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$'
re_jwt='^[A-Za-z0-9+/=_.~-]{32,}$'
re_redis='^[A-Za-z0-9_.~-]{16,}$'
re_dbpw='^[A-Za-z0-9_.~-]{12,128}$'
re_dbuser='^[a-zA-Z][a-zA-Z0-9_]{0,29}$'
[[ "$DOCKERHUB_USER" =~ $re_user ]] || die "DOCKERHUB_USER must be 4-30 lowercase letters/digits"
[[ "$IMAGE_TAG" =~ $re_tag ]] || die "IMAGE_TAG is not a valid Docker tag: $IMAGE_TAG"
[[ "$JWT_SECRET" =~ $re_jwt ]] || die "JWT_SECRET needs >= 32 characters from A-Za-z0-9+/=_.~- (try: openssl rand -hex 32)"
[[ "$REDIS_PASSWORD" =~ $re_redis ]] || die "REDIS_PASSWORD needs >= 16 characters from A-Za-z0-9_.~- (try: openssl rand -hex 24)"
[[ "$DB_PASSWORD" =~ $re_dbpw ]] || die "DB_PASSWORD needs 12-128 characters from A-Za-z0-9_.~- (try: openssl rand -hex 16)"
[[ "$DB_USERNAME" =~ $re_dbuser ]] || die "DB_USERNAME must start with a letter; letters, digits and _ only"

KEY="${SSH_KEY_PATH:-$HOME/.ssh/keys/rwp_remotebuild.pem}"
[[ -r "$KEY" ]] || die "SSH key is missing or unreadable: $KEY"
REMOTE_USER="ec2-user"

TF_BIN="${TF_BIN:-tofu}"
for tool in "$TF_BIN" jq ssh scp curl; do
  command -v "$tool" >/dev/null 2>&1 || die "'$tool' not found in PATH"
done

# --- 1. Which IP may SSH in ---------------------------------------------------
if [[ -z "${SSH_CIDR:-}" ]]; then
  MY_IP="$(curl -4 -fsS https://checkip.amazonaws.com | tr -d '[:space:]')"
  [[ "$MY_IP" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "could not detect public IPv4 address (got: '$MY_IP')"
  SSH_CIDR="$MY_IP/32"
fi
echo "Allowing SSH from $SSH_CIDR"

# Secrets reach OpenTofu through the environment, not through command-line arguments.
export TF_VAR_ssh_cidr="$SSH_CIDR"
export TF_VAR_db_username="$DB_USERNAME"
export TF_VAR_db_password="$DB_PASSWORD"
if [[ -n "${AWS_REGION:-}" ]]; then
  export TF_VAR_aws_region="$AWS_REGION"
fi

# --- 2. Provision ---------------------------------------------------------------
REMOTE_ENV=""
cleanup() {
  rm -f -- tfplan ${REMOTE_ENV:+"$REMOTE_ENV"}
}
trap cleanup EXIT

"$TF_BIN" init -input=false
"$TF_BIN" plan -input=false -out=tfplan

if ((!AUTO_APPROVE)); then
  read -r -p "Apply this plan? (a new RDS instance can take ~10 minutes) [y/N] " answer
  [[ "$answer" == "y" || "$answer" == "Y" ]] || die "cancelled; nothing was applied"
fi
"$TF_BIN" apply -input=false tfplan

IP_ADDRESS="$("$TF_BIN" output -raw ec2_public_ip)"
DB_HOST="$("$TF_BIN" output -raw db_address)"
[[ -n "$IP_ADDRESS" && -n "$DB_HOST" ]] || die "missing outputs from $TF_BIN (ec2_public_ip / db_address)"
echo "Server IP: $IP_ADDRESS"
echo "Database:  $DB_HOST"

REMOTE="$REMOTE_USER@$IP_ADDRESS"
SSH_OPTS=(
  -i "$KEY"
  -o StrictHostKeyChecking=accept-new
  -o UserKnownHostsFile="$SCRIPT_DIR/known_hosts"
  -o BatchMode=yes
  -o ConnectTimeout=5
  -o ServerAliveInterval=30
)

echo "Waiting for SSH to become available"
attempts=0
until ssh "${SSH_OPTS[@]}" "$REMOTE" true 2>/dev/null; do
  attempts=$((attempts + 1))
  ((attempts < 60)) || die "SSH not reachable after $attempts attempts"
  sleep 5
done
echo "SSH is up"

# --- 3. Build the server-side .env ----------------------------------------------
# Passwords are restricted to URL-safe characters, and the DB password is still
# percent-encoded for the connection URL as a second line of defence.
umask 077
REMOTE_ENV="$(mktemp)"
ENCODED_DB_PASSWORD="$(jq -rn --arg p "$DB_PASSWORD" '$p | @uri')"
cat >"$REMOTE_ENV" <<EOF
DOCKERHUB_USER=$DOCKERHUB_USER
IMAGE_TAG=$IMAGE_TAG
JWT_SECRET=$JWT_SECRET
REDIS_PASSWORD=$REDIS_PASSWORD
TO_DO_DB_URL=postgres://$DB_USERNAME:$ENCODED_DB_PASSWORD@$DB_HOST:5432/to_do
AUTH_DB_URL=postgres://$DB_USERNAME:$ENCODED_DB_PASSWORD@$DB_HOST:5432/auth
DB_HOST=$DB_HOST
DB_USERNAME=$DB_USERNAME
PGPASSWORD=$DB_PASSWORD
EOF

# --- 4. Upload configuration ---------------------------------------------------
echo "Uploading configuration"
ssh "${SSH_OPTS[@]}" "$REMOTE" 'install -d -m 700 "$HOME/app"'
scp "${SSH_OPTS[@]}" "$REMOTE_ENV" "$REMOTE:app/.env"
scp "${SSH_OPTS[@]}" docker-compose.yml nginx.conf init-databases.sql "$REMOTE:app/"
ssh "${SSH_OPTS[@]}" "$REMOTE" 'chmod 600 "$HOME/app/.env"'

# Only needed when the Docker Hub repositories are private. The token travels over stdin.
if [[ -n "${DOCKERHUB_TOKEN:-}" ]]; then
  printf '%s' "$DOCKERHUB_TOKEN" |
    ssh "${SSH_OPTS[@]}" "$REMOTE" "sudo docker login -u $(printf '%q' "$DOCKERHUB_USER") --password-stdin"
fi

# --- 5. Deploy -------------------------------------------------------------------
# The quoted 'EOF' makes this heredoc run on the REMOTE shell, with no local expansion.
ssh "${SSH_OPTS[@]}" "$REMOTE" "bash -s" <<'EOF'
set -euo pipefail

# Wait for the marker file that server_build.sh creates when provisioning is done.
for _ in $(seq 1 100); do
  [[ -f "$HOME/output.txt" ]] && break
  echo "waiting for server setup to finish"
  sleep 3
done
[[ -f "$HOME/output.txt" ]] || { echo "error: setup marker never appeared" >&2; exit 1; }

cd "$HOME/app"

# RDS only creates `to_do`; create `auth` too. PGPASSWORD/DB_HOST/DB_USERNAME come from .env.
for attempt in $(seq 1 10); do
  if sudo docker run --rm --env-file .env -v "$PWD/init-databases.sql:/init.sql:ro" \
      postgres:17-alpine \
      sh -c 'psql -h "$DB_HOST" -U "$DB_USERNAME" -d to_do -v ON_ERROR_STOP=1 -f /init.sql'; then
    break
  fi
  [[ "$attempt" -lt 10 ]] || { echo "error: could not reach the database" >&2; exit 1; }
  echo "database not ready (attempt $attempt), retrying"
  sleep 10
done

sudo docker compose pull
sudo docker compose up -d --remove-orphans
sudo docker compose ps
EOF

# --- 6. Health check ---------------------------------------------------------------
echo "Waiting for the application to answer on http://$IP_ADDRESS/healthz"
attempts=0
until curl -fsS --max-time 5 "http://$IP_ADDRESS/healthz" >/dev/null 2>&1; do
  attempts=$((attempts + 1))
  if ((attempts >= 30)); then
    echo "error: the application did not become healthy." >&2
    echo "  Logs: ssh -i $KEY $REMOTE 'cd app && sudo docker compose logs --tail=100'" >&2
    exit 1
  fi
  sleep 5
done

cat <<EOF

Deployment finished.
  App:          http://$IP_ADDRESS/
  Create user:  INGRESS_URL=http://$IP_ADDRESS ./scripts/create_user you@example.com
  Logs:         ssh -i $KEY $REMOTE 'cd app && sudo docker compose logs -f'
  Tear down:    ./scripts/deployment_cleanup.sh --prod
EOF
