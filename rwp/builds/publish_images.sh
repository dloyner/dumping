#!/usr/bin/env bash
# builds/publish_images.sh
# Build the production Docker images on the remote build server and push them to Docker Hub:
#   <user>/compute-unit  (ingress binary with the embedded frontend; Dockerfile.x86)
#   <user>/redis-cache   (Redis 7 with the session module)
#
# Usage:
#   DOCKERHUB_USER=alice DOCKERHUB_TOKEN=dckr_pat_... ./publish_images.sh
# Optional:
#   IMAGE_TAG=v1     tag to push (default: git short SHA, or a timestamp outside git); `latest` is pushed too
#   SSH_CIDR=...     CIDR allowed to SSH in (default: your detected public IPv4 /32)
#   SSH_KEY_PATH=... private key (default: ~/.ssh/keys/rwp_remotebuild.pem)
#   TF_BIN=terraform use Terraform instead of OpenTofu
# The build server is provisioned automatically when it does not exist yet.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
PROJECT_NAME="$(basename -- "$PROJECT_ROOT")"
PROJECT_PARENT="$(dirname -- "$PROJECT_ROOT")"
cd -- "$SCRIPT_DIR"

die() {
  echo "error: $*" >&2
  exit 1
}

# --- 0. Inputs ---------------------------------------------------------------
DOCKERHUB_USER="${DOCKERHUB_USER:-}"
[[ -n "$DOCKERHUB_USER" ]] || die "set DOCKERHUB_USER (your Docker Hub username)"
re_user='^[a-z0-9]{4,30}$'
[[ "$DOCKERHUB_USER" =~ $re_user ]] || die "DOCKERHUB_USER must be 4-30 lowercase letters/digits"

if [[ -z "${DOCKERHUB_TOKEN:-}" ]]; then
  read -r -s -p "Docker Hub access token for $DOCKERHUB_USER: " DOCKERHUB_TOKEN
  echo
fi
[[ -n "$DOCKERHUB_TOKEN" ]] || die "a Docker Hub access token is required"

if [[ -z "${IMAGE_TAG:-}" ]]; then
  if IMAGE_TAG="$(git -C "$PROJECT_ROOT" rev-parse --short HEAD 2>/dev/null)"; then
    if [[ -n "$(git -C "$PROJECT_ROOT" status --porcelain 2>/dev/null)" ]]; then
      IMAGE_TAG="$IMAGE_TAG-dirty"
    fi
  else
    IMAGE_TAG="$(date +%Y%m%d%H%M)"
  fi
fi
re_tag='^[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}$'
[[ "$IMAGE_TAG" =~ $re_tag ]] || die "IMAGE_TAG is not a valid Docker tag: $IMAGE_TAG"

KEY="${SSH_KEY_PATH:-$HOME/.ssh/keys/rwp_remotebuild.pem}"
[[ -r "$KEY" ]] || die "SSH key is missing or unreadable: $KEY"
REMOTE_USER="ec2-user"

TF_BIN="${TF_BIN:-tofu}"
command -v "$TF_BIN" >/dev/null 2>&1 || die "'$TF_BIN' not found in PATH (try: brew install opentofu)"

# --- 1. Make sure the build server exists --------------------------------------
IP_ADDRESS="$("$TF_BIN" output -raw ec2_public_ip 2>/dev/null || true)"
if [[ -z "$IP_ADDRESS" ]]; then
  echo "No build server found; provisioning one"
  if [[ -z "${SSH_CIDR:-}" ]]; then
    MY_IP="$(curl -4 -fsS https://checkip.amazonaws.com | tr -d '[:space:]')"
    [[ "$MY_IP" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "could not detect public IPv4 address (got: '$MY_IP')"
    SSH_CIDR="$MY_IP/32"
  fi
  echo "Allowing SSH from $SSH_CIDR"
  "$TF_BIN" init -input=false
  "$TF_BIN" apply -input=false -auto-approve -var "ssh_cidr=$SSH_CIDR"
  IP_ADDRESS="$("$TF_BIN" output -raw ec2_public_ip)"
fi
[[ -n "$IP_ADDRESS" ]] || die "no public IP in $TF_BIN output"
echo "Build server: $IP_ADDRESS"

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
  ((attempts < 40)) || die "SSH not reachable after $attempts attempts (did your IP change? re-run with SSH_CIDR set and apply)"
  sleep 5
done
echo "SSH is up"

# --- 2. Send the token (via stdin, never on a command line) and the source --------
printf '%s' "$DOCKERHUB_TOKEN" |
  ssh "${SSH_OPTS[@]}" "$REMOTE" 'umask 077 && cat > "$HOME/.dockerhub_token"'

echo "Sending project source"
COPYFILE_DISABLE=1 tar -czf - \
  --exclude='*/node_modules' --exclude='*/target' --exclude='*/.git' \
  --exclude='*/.env' --exclude='*/.env.production' --exclude='*/.DS_Store' \
  --exclude='*/aws_access_key.txt' --exclude='*.pem' \
  --exclude='*/builds/.terraform' --exclude='*/builds/terraform.tfstate*' \
  --exclude='*/builds/known_hosts' --exclude='*/builds/tfplan' --exclude='*/builds/ingress.bin' \
  --exclude='*/deployment/.terraform' --exclude='*/deployment/terraform.tfstate*' \
  --exclude='*/deployment/known_hosts' --exclude='*/deployment/tfplan' \
  -C "$PROJECT_PARENT" "$PROJECT_NAME" |
  ssh "${SSH_OPTS[@]}" "$REMOTE" 'tar -xzf - -C "$HOME"'

# --- 3. Build and push on the server ----------------------------------------------
# The quoted 'EOF' makes the heredoc run on the REMOTE shell, with no local expansion.
ssh "${SSH_OPTS[@]}" "$REMOTE" \
  "bash -s -- $(printf '%q' "$PROJECT_NAME") $(printf '%q' "$DOCKERHUB_USER") $(printf '%q' "$IMAGE_TAG")" <<'EOF'
set -euo pipefail
PROJECT_NAME=$1
HUB_USER=$2
TAG=$3

# Always remove the token file, even when the build fails.
trap 'rm -f "$HOME/.dockerhub_token"; sudo docker logout >/dev/null 2>&1 || true' EXIT

# Wait for the marker file that build_server.sh creates when provisioning is done.
for _ in $(seq 1 100); do
  [[ -f "$HOME/output.txt" ]] && break
  echo "waiting for server setup to finish"
  sleep 3
done
[[ -f "$HOME/output.txt" ]] || { echo "error: setup marker never appeared" >&2; exit 1; }
sudo systemctl is-active --quiet docker || { echo "error: docker is not running (was build_server.sh updated?)" >&2; exit 1; }

sudo docker login -u "$HUB_USER" --password-stdin < "$HOME/.dockerhub_token"

cd "$HOME/$PROJECT_NAME"

echo "==> Building compute-unit"
sudo docker build --platform linux/amd64 -f Dockerfile.x86 \
  -t "$HUB_USER/compute-unit:$TAG" -t "$HUB_USER/compute-unit:latest" .

echo "==> Building redis-cache"
sudo docker build -f nanoservices/user-session-cache/cache-module/Dockerfile \
  -t "$HUB_USER/redis-cache:$TAG" -t "$HUB_USER/redis-cache:latest" .

echo "==> Pushing"
for image in compute-unit redis-cache; do
  sudo docker push "$HUB_USER/$image:$TAG"
  sudo docker push "$HUB_USER/$image:latest"
done
EOF

cat <<EOF

Published:
  $DOCKERHUB_USER/compute-unit:$IMAGE_TAG
  $DOCKERHUB_USER/redis-cache:$IMAGE_TAG

Next: set IMAGE_TAG=$IMAGE_TAG in deployment/.env.production, then run deployment/run_deployment.sh
The build server keeps billing until you run: ./scripts/deployment_cleanup.sh --tofu
EOF
