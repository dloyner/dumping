#!/usr/bin/env bash
# scripts/deployment_cleanup.sh
# Tear down everything the deployment layouts (monolith / microservice) and the
# remote build server created. Safe by default: with no options it only stops
# and removes containers and networks, keeping database volumes and images.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT"

usage() {
  cat <<'EOF'
Usage: scripts/deployment_cleanup.sh [options]

Always (default):
  Stop and remove every container and network of the Compose project, for both
  layouts (monolith and microservice), plus orphans. Volumes are kept.

Options:
  --volumes      also delete Docker volumes (this DELETES PostgreSQL data)
  --images       also remove built images (compute-unit, cache, frontend, CI leftovers)
  --tofu         run `tofu destroy` in builds/ to remove the EC2 build server
                 (set TF_BIN=terraform to use Terraform, SSH_CIDR / AWS_REGION to override)
  --prod         run `tofu destroy` in deployment/: DELETES the production EC2 server
                 and the RDS database with all its data. Never part of --all.
  --local-files  delete builds/{known_hosts,ingress.bin,tfplan} and
                 deployment/{known_hosts,tfplan} (terraform.tfstate is never deleted)
  --all          --volumes --images --tofu --local-files (not --prod)
  --dry-run      print the commands without running them
  --yes          do not ask for confirmation
  -h, --help     show this help
EOF
}

VOLUMES=0
IMAGES=0
TOFU=0
PROD=0
LOCAL_FILES=0
DRY_RUN=0
ASSUME_YES=0

while (($#)); do
  case "$1" in
    --volumes) VOLUMES=1 ;;
    --images) IMAGES=1 ;;
    --tofu) TOFU=1 ;;
    --prod) PROD=1 ;;
    --local-files) LOCAL_FILES=1 ;;
    --all)
      VOLUMES=1
      IMAGES=1
      TOFU=1
      LOCAL_FILES=1
      ;;
    --dry-run) DRY_RUN=1 ;;
    --yes) ASSUME_YES=1 ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

run() {
  if ((DRY_RUN)); then
    printf '[dry-run]'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

# --- Summary and confirmation ------------------------------------------------
echo "Deployment cleanup will:"
echo "  - stop and remove all Compose containers/networks (monolith + microservice)"
((VOLUMES)) && echo "  - DELETE Docker volumes, including PostgreSQL data"
((IMAGES)) && echo "  - remove built Docker images"
((TOFU)) && echo "  - DESTROY the EC2 build server with ${TF_BIN:-tofu} (builds/)"
((PROD)) && echo "  - DESTROY the PRODUCTION server and the RDS DATABASE (deployment/) with ${TF_BIN:-tofu}"
((LOCAL_FILES)) && echo "  - delete local build/deployment artifacts (known_hosts, ingress.bin, tfplan)"
echo "Kept: .env, .env.production, terraform.tfstate, source code$( ((VOLUMES)) || echo ', database volumes')."

if ((!ASSUME_YES && !DRY_RUN)); then
  read -r -p "Type CLEANUP to continue: " confirmation
  if [[ "$confirmation" != "CLEANUP" ]]; then
    echo "Cancelled; nothing was changed."
    exit 1
  fi
fi

# --- 1. Docker Compose -------------------------------------------------------
if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found in PATH" >&2
  exit 1
fi

# The fragments require JWT_SECRET during interpolation, even for `down`.
export JWT_SECRET="${JWT_SECRET:-cleanup-placeholder}"

COMPOSE=(docker compose)
if [[ -f .env ]]; then
  COMPOSE+=(--env-file .env)
fi
COMPOSE+=(
  -f compose.yaml
  -f ingress/build-config.yml
  -f nanoservices/auth/build-config.yml
  -f nanoservices/to_do/build-config.yml
  -f frontend/build-config.yml
)

DOWN_ARGS=(--remove-orphans)
((VOLUMES)) && DOWN_ARGS+=(--volumes)
((IMAGES)) && DOWN_ARGS+=(--rmi local)

echo "==> Stopping Compose project"
run "${COMPOSE[@]}" down "${DOWN_ARGS[@]}"

if ((IMAGES)); then
  echo "==> Removing leftover containers and images"
  if docker container inspect cache-ci >/dev/null 2>&1; then
    run docker rm -f cache-ci
  fi
  # compute-unit has a custom tag, so `--rmi local` does not remove it.
  for image in compute-unit cache-module-ci; do
    if docker image inspect "$image" >/dev/null 2>&1; then
      run docker image rm "$image"
    fi
  done
fi

# --- 2. OpenTofu / Terraform -------------------------------------------------
if ((TOFU)); then
  TF_BIN="${TF_BIN:-tofu}"
  REGION="${AWS_REGION:-ap-southeast-1}"
  if ! command -v "$TF_BIN" >/dev/null 2>&1; then
    echo "error: '$TF_BIN' not found in PATH" >&2
    exit 1
  fi
  if [[ ! -f builds/terraform.tfstate ]]; then
    echo "==> No builds/terraform.tfstate found; nothing to destroy"
  else
    echo "==> Destroying the build server with $TF_BIN"
    # ssh_cidr only has to pass validation; destroy works from the state file.
    run "$TF_BIN" -chdir=builds init -input=false
    run "$TF_BIN" -chdir=builds destroy -input=false -auto-approve \
      -var "ssh_cidr=${SSH_CIDR:-203.0.113.1/32}"

    if ((!DRY_RUN)) && command -v aws >/dev/null 2>&1; then
      left="$(aws ec2 describe-instances --region "$REGION" \
        --filters "Name=tag:Name,Values=to-do build server" \
        "Name=instance-state-name,Values=pending,running,stopping,stopped" \
        --query 'Reservations[].Instances[].InstanceId' --output text || true)"
      if [[ -n "$left" ]]; then
        echo "WARNING: build server instance(s) still exist in $REGION: $left" >&2
      else
        echo "Verified: no build server instances remain in $REGION"
      fi
    fi
  fi
fi

# --- 2b. Production stack (EC2 + RDS) ----------------------------------------
if ((PROD)); then
  TF_BIN="${TF_BIN:-tofu}"
  REGION="${AWS_REGION:-ap-southeast-1}"
  if ! command -v "$TF_BIN" >/dev/null 2>&1; then
    echo "error: '$TF_BIN' not found in PATH" >&2
    exit 1
  fi
  if [[ ! -f deployment/terraform.tfstate ]]; then
    echo "==> No deployment/terraform.tfstate found; nothing to destroy"
  else
    echo "==> Destroying the production stack with $TF_BIN"
    # destroy works from the state file; these values only have to pass variable validation.
    run "$TF_BIN" -chdir=deployment init -input=false
    run env "TF_VAR_ssh_cidr=${SSH_CIDR:-203.0.113.1/32}" \
      "TF_VAR_db_password=${DB_PASSWORD:-destroy-placeholder-0}" \
      "TF_VAR_domain_name=${DOMAIN_NAME:?set DOMAIN_NAME}" \
      ${AWS_REGION:+"TF_VAR_aws_region=$AWS_REGION"} \
      "$TF_BIN" -chdir=deployment destroy -input=false -auto-approve

    if ((!DRY_RUN)) && command -v aws >/dev/null 2>&1; then
      servers="$(aws ec2 describe-instances --region "$REGION" \
        --filters "Name=tag:Name,Values=to-do production server" \
        "Name=instance-state-name,Values=pending,running,stopping,stopped" \
        --query 'Reservations[].Instances[].InstanceId' --output text || true)"
      databases="$(aws rds describe-db-instances --region "$REGION" \
        --query "DBInstances[?starts_with(DBInstanceIdentifier, 'todo-')].DBInstanceIdentifier" \
        --output text || true)"
      if [[ -n "$servers$databases" ]]; then
        echo "WARNING: resources still exist in $REGION: $servers $databases" >&2
      else
        echo "Verified: no production server or database remains in $REGION"
      fi
    fi
  fi
fi

# --- 3. Local build artifacts ------------------------------------------------
if ((LOCAL_FILES)); then
  echo "==> Removing local build artifacts"
  run rm -f builds/known_hosts builds/ingress.bin builds/tfplan deployment/known_hosts deployment/tfplan
fi

echo "Cleanup finished."
