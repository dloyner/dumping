#!/usr/bin/env bash
# builds/run_build.sh
# Provision the build server with OpenTofu, copy the project over, build it remotely.

# Stop on errors, undefined variables, and failures inside pipes
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
PROJECT_NAME="$(basename -- "$PROJECT_ROOT")"
PROJECT_PARENT="$(dirname -- "$PROJECT_ROOT")"
cd -- "$SCRIPT_DIR"

KEY="${SSH_KEY_PATH:-$HOME/.ssh/keys/rwp_remotebuild.pem}"
REMOTE_USER="ec2-user"
if [[ ! -r "$KEY" ]]; then
  echo "error: SSH key is missing or unreadable: $KEY" >&2
  exit 1
fi

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
# Send the complete Cargo workspace and frontend source without local artifacts
# or the ignored .env file, which contains local credentials.
echo "Sending project source"
COPYFILE_DISABLE=1 tar -czf - \
  --exclude='*/node_modules' --exclude='*/target' --exclude='*/.git' \
  --exclude='*/.env' --exclude='*/.DS_Store' \
  --exclude='*/aws_access_key.txt' --exclude='*.pem' \
  --exclude='*/builds/.terraform' --exclude='*/builds/terraform.tfstate*' \
  --exclude='*/builds/known_hosts' \
  -C "$PROJECT_PARENT" "$PROJECT_NAME" \
  | ssh "${SSH_OPTS[@]}" "$REMOTE" 'tar -xzf - -C "$HOME"'

# --- 4. Build on the server --------------------------------------------------
# The quoted 'EOF' makes the heredoc run on the REMOTE shell, with no local expansion
ssh "${SSH_OPTS[@]}" "$REMOTE" "bash -s -- $(printf '%q' "$PROJECT_NAME")" <<'EOF'
set -euo pipefail
PROJECT_NAME=$1

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
cargo install wasm-pack

# Build the embedded frontend assets from the workspace root.
export PATH="/usr/local/bin:$PATH"
bun --version
cd "$HOME/$PROJECT_NAME/frontend"
bun install --frozen-lockfile
bun run wasm
bun run build
cd "$HOME/$PROJECT_NAME"
cargo build --release -p ingress
EOF

# --- 5. Fetch the release binary ---------------------------------------------
scp "${SSH_OPTS[@]}" "$REMOTE:$PROJECT_NAME/target/release/ingress" "$SCRIPT_DIR/ingress.bin"
echo "Binary saved to $SCRIPT_DIR/ingress.bin"

echo "Build finished on $IP_ADDRESS"
