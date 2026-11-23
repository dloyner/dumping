#!/usr/bin/env bash
# builds/run_build.sh
# Provision the build server with OpenTofu, copy the project over, build it remotely.

# Stop on errors, undefined variables, and failures inside pipes
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
cd -- "$SCRIPT_DIR"

KEY="$HOME/.ssh/keys/remotebuild.pem"
REMOTE_USER="ec2-user"

# IaC binary: defaults to OpenTofu, override with `TF_BIN=terraform ./run_build.sh`
TF_BIN="${TF_BIN:-tofu}"
if ! command -v "$TF_BIN" >/dev/null 2>&1; then
  echo "error: '$TF_BIN' not found in PATH (try: brew install opentofu)" >&2
  exit 1
fi

# --- 0. Work out which IP may SSH in ----------------------------------------
# Override with SSH_CIDR=203.0.113.7/32 if auto-detection is wrong (VPN, IPv6, ...)
if [[ -z "${SSH_CIDR:-}" ]]; then
  MY_IP="$(curl -4 -fsS https://checkip.amazonaws.com | tr -d '[:space:]')"
  if [[ ! "$MY_IP" =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "error: could not detect public IPv4 address (got: '$MY_IP')" >&2
    exit 1
  fi
  SSH_CIDR="$MY_IP/32"
fi
echo "Allowing SSH from $SSH_CIDR"

# --- 1. Provision ----------------------------------------------------------
"$TF_BIN" init -input=false
"$TF_BIN" apply -input=false -auto-approve -var "ssh_cidr=$SSH_CIDR"

# -raw prints the bare string, so no jq is needed
IP_ADDRESS="$("$TF_BIN" output -raw ec2_public_ip)"
if [[ -z "$IP_ADDRESS" ]]; then
  echo "error: no public IP in $TF_BIN output" >&2
  exit 1
fi
echo "Server IP: $IP_ADDRESS"

REMOTE="$REMOTE_USER@$IP_ADDRESS"

# Shared options for both ssh and scp
SSH_OPTS=(
  -i "$KEY"
  -o StrictHostKeyChecking=accept-new
  -o UserKnownHostsFile="$SCRIPT_DIR/known_hosts"
  -o BatchMode=yes
  -o ConnectTimeout=5
  -o ServerAliveInterval=30
)

# --- 2. Wait until SSH is reachable -----------------------------------------
echo "Waiting for SSH to become available"
attempts=0
until ssh "${SSH_OPTS[@]}" "$REMOTE" true 2>/dev/null; do
  attempts=$((attempts + 1))
  if (( attempts >= 40 )); then
    echo "error: SSH not reachable after $attempts attempts" >&2
    exit 1
  fi
  sleep 5
done
echo "SSH is up"

# --- 3. Copy the project -----------------------------------------------------
# Stream a tarball over ssh so build artifacts are excluded without touching local files
send_dir() {
  local dir="$1"
  # COPYFILE_DISABLE stops macOS tar from adding ._ metadata files
  COPYFILE_DISABLE=1 tar -czf - \
    --exclude=node_modules --exclude=target --exclude=.git \
    -C .. "$dir" \
    | ssh "${SSH_OPTS[@]}" "$REMOTE" 'tar -xzf - -C "$HOME"'
}

for dir in nanoservices ingress glue; do
  echo "Sending $dir"
  send_dir "$dir"
done

scp "${SSH_OPTS[@]}" ../compose.yaml ../.env "$REMOTE:/home/$REMOTE_USER/"

# --- 4. Build on the server --------------------------------------------------
# The quoted 'EOF' makes the heredoc run on the REMOTE shell, with no local expansion
ssh "${SSH_OPTS[@]}" "$REMOTE" 'bash -s' <<'EOF'
set -euo pipefail

# Wait for the marker file that build_server.sh creates when provisioning is done
for _ in $(seq 1 100); do
  [[ -f "$HOME/output.txt" ]] && break
  echo "waiting for file"
  sleep 3
done
[[ -f "$HOME/output.txt" ]] || { echo "error: setup marker never appeared" >&2; exit 1; }
echo "File found"

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Load .env into the environment (handles quotes and spaces, unlike xargs)
chmod 600 .env
set -a
source .env
set +a

cd ingress/frontend
npm install
npm run build
cd ..
cargo build --release
EOF

echo "Build finished on $IP_ADDRESS"
