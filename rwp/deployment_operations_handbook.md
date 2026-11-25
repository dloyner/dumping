# DevOps Handbook — Docker, Docker Hub và AWS

This guide takes you from a fresh checkout to a running, HTTPS-protected
deployment. For what the application is and how to run it for development, see
[`README.md`](README.md).

## Contents

1. [Overview](#1-overview)
2. [Prerequisites](#2-prerequisites)
3. [Docker and Compose layouts (local)](#3-docker-and-compose-layouts-local)
4. [AWS production deployment](#4-aws-production-deployment)
5. [HTTP and HTTPS: how traffic is secured](#5-http-and-https-how-traffic-is-secured)
6. [Nginx configurations explained](#6-nginx-configurations-explained)
7. [Operations](#7-operations)
8. [Tear down](#8-tear-down)
9. [Troubleshooting](#9-troubleshooting)
10. [Pre-launch checklist](#10-pre-launch-checklist)

---

## 1. Overview

There are three ways to run the containers. All of them use the same image,
`compute-unit`, built from the root `Dockerfile`; only the command and the
surrounding services differ.

| Path | Compose files / tooling | TLS | Section |
|---|---|---|---|
| Monolith, local | `compose.yaml` + `ingress/build-config.yml` | Self-signed certificate on Nginx | [3.1](#31-monolith-ingress--nginx-with-https) |
| Microservices, local | `compose.yaml` + `nanoservices/*/build-config.yml` + `frontend/build-config.yml` | None (HTTP) | [3.2](#32-microservices) |
| AWS production | `builds/` and `deployment/` (OpenTofu + scripts) | ACM certificate on an ALB | [4](#4-aws-production-deployment) |

The monolith and microservice layouts use different process layouts and must
**not** be combined in one Compose command.

### Production architecture

```text
                    Internet
                       |
            DNS: todo.example.com  (CNAME / alias)
                       |
                       v
        +----------------------------+
        | Application Load Balancer  |  :80  -> 301 redirect to :443
        |  (ACM certificate, TLS 1.3 |  :443 -> forward
        |   /1.2 policy)             |
        +-------------|--------------+
                      | HTTP :80   (security group: ALB -> web server only)
                      v
   +------------------------- EC2 (Amazon Linux 2023, Elastic IP) ----------+
   |  Docker Compose project "todo"                                          |
   |    nginx  --->  ingress :8001  --->  cache (Redis + session module)     |
   +-------------------------|-----------------------------------------------+
                             | PostgreSQL over TLS :5432
                             v
               +---------------------------+
               | RDS PostgreSQL (private)  |  databases: to_do, auth
               +---------------------------+
```

Images are **built on a temporary remote build server**, pushed to Docker Hub,
and pulled by the production EC2 instance:

```text
 your machine --ssh--> build server (EC2) --docker push--> Docker Hub
 your machine --tofu/ssh--> production EC2 --docker pull--> Docker Hub
```

### What the production stack creates

| Resource | Details |
|---|---|
| Load balancer | Application Load Balancer, HTTP→HTTPS redirect, HTTPS listener with an ACM certificate, target group health check on `/healthz` |
| Web server | EC2 `t3.small`, 20 GiB encrypted gp3 root volume, IMDSv2 required, Elastic IP |
| Database | RDS PostgreSQL 18, `db.t3.micro`, 20 GiB encrypted gp3, private, single-AZ, TLS required |
| Security groups | ALB: 80/443 from anywhere. Web server: 80 from the ALB only, 22 from your IP only. Database: 5432 from the web server only |
| Build server (separate stack) | EC2 `m7i-flex.large`, 60 GiB encrypted volume, used only to build images; it has its own public IP and SSH allowlist |

Rough cost: the ALB, EC2/RDS instances, EBS volumes, public IPv4 addresses and
RDS snapshots can continue to incur charges. Tear the build server down when
you are not building (see [8](#8-tear-down)); inspect the account for unattached
EIPs and snapshots after teardown.

---

## 2. Prerequisites

### Accounts and domain

- A **Docker Hub** account and an access token (Account settings → Personal
  access token, read/write).
- An **AWS account** with credentials configured for the CLI
  (`aws configure` or SSO). The identity needs permission to manage EC2, ELBv2,
  RDS, security groups and Elastic IPs, to read ACM certificates, and to read
  SSM public parameters (used to look up the Amazon Linux AMI). Add Route 53
  permissions if you use Route 53.
- A **domain name** you can add DNS records to. ACM cannot issue certificates for
  IP addresses or `amazonaws.com` names.
- The AWS region is `ap-southeast-1` by default. The ACM certificate and the ALB
  **must be in the same region**.

### Tools on your machine

| Tool | Used for |
|---|---|
| Docker (with BuildKit) | Local images and Compose layouts |
| OpenTofu (`tofu`) or Terraform (`TF_BIN=terraform`), version ≥ 1.10 (the configured S3 backend uses `use_lockfile`) | Provisioning |
| `aws` CLI | Key pair and certificate setup, diagnostics |
| `ssh`, `scp`, `curl`, `jq` | Deployment scripts |
| `xh` | Creating the first user |
| `openssl` | Generating secrets and the dev certificate |

### EC2 key pair

The scripts expect an EC2 key pair named `rwp_remotebuild` in the region and its
private key at `~/.ssh/keys/rwp_remotebuild.pem` (override with `SSH_KEY_PATH`
and the `ssh_key_name` variable).

```sh
mkdir -p ~/.ssh/keys
aws ec2 create-key-pair --region ap-southeast-1 --key-name rwp_remotebuild \
  --query KeyMaterial --output text > ~/.ssh/keys/rwp_remotebuild.pem
chmod 400 ~/.ssh/keys/rwp_remotebuild.pem
```

### Secrets

Generate every secret with hex output; it is safe inside URLs:

```sh
openssl rand -hex 32   # JWT_SECRET
openssl rand -hex 24   # REDIS_PASSWORD
openssl rand -hex 16   # DB_PASSWORD, APP_DB_PASSWORD
```

---

## 3. Docker and Compose layouts (local)

### 3.0 The images

**`compute-unit`** (root `Dockerfile`) is a multi-stage build:

1. Builds the frontend with Bun and `wasm-pack` (the WebAssembly helper), so the
   result can be embedded in the ingress binary.
2. Builds three release binaries (`ingress`, `auth-axum-server`,
   `to-do-axum-server`, the last one with `--no-default-features --features
   auth-http`) and strips them. BuildKit cache mounts keep the Cargo registry and
   `target/` between builds.
3. Copies only the binaries into `gcr.io/distroless/cc-debian12:nonroot`: no
   shell, no package manager, non-root user. The default command is `ingress`.

The same `Dockerfile` builds for both CPU architectures; select one with
`--platform`:

```sh
docker build --platform linux/amd64 -t compute-unit .    # x86_64
docker build --platform linux/arm64 -t compute-unit .    # ARM64 (uses emulation on x86 hosts)
```

`INGRESS_URL` can be passed as a build argument
(`--build-arg INGRESS_URL=...`); it only matters for the `localhost:3000` dev
server, because the built frontend otherwise calls its own origin.

**`redis-cache`** (`nanoservices/user-session-cache/cache-module/Dockerfile`)
compiles the Redis module and puts it into the official Redis image:

```sh
docker build -f nanoservices/user-session-cache/cache-module/Dockerfile -t redis-cache .
```

Containers started from it must load the module and set a password:
`redis-server --loadmodule ./libcache_module.so --requirepass <password>`. The
Compose files do this for you.

### 3.1 Monolith: ingress + Nginx with HTTPS

One `ingress` process (frontend, auth and to-do routers) behind an Nginx
container that terminates TLS with a self-signed certificate.

1. Create `.env` (see the README) and generate the development certificate:

   ```sh
   ./scripts/gen_dev_cert.sh
   ```

   This writes `ingress/self.crt` and `ingress/self.key` (valid 365 days, for
   `localhost`/`127.0.0.1`). Both are excluded from Git and from the Docker build
   context; the generated private key must not be committed.

2. Start the stack from the repository root:

   ```sh
   docker compose --env-file .env \
     -f compose.yaml -f ingress/build-config.yml up --build
   ```

   This starts PostgreSQL, Redis (with the session module), ingress and Nginx.
   Ingress runs both database migrations on startup.

3. Open <https://localhost> (accept the browser warning for the self-signed
   certificate). Before creating a user, set `INGRESS_URL=https://localhost`
   inside the repository-root `.env`, then run
   `./scripts/create_user.sh you@example.com` (the helper needs `xh`). The helper
   sources `.env` after process startup, so a one-off shell prefix for
   `INGRESS_URL` is overwritten when `.env` already defines it. The helper also
   does not expose an insecure-TLS flag; if `xh` rejects the self-signed
   certificate, trust the generated certificate in your local client or use a
   separately invoked HTTP client with an explicit development-only certificate
   override.

Notes:

- Port 80 redirects to HTTPS. Ingress is also published on `127.0.0.1:8001`
  only, for debugging; it is not reachable from other machines.
- The fragment sets `TRUST_PROXY=1`, so rate limiting uses the client IP that
  Nginx passes in `X-Real-IP`.
- `ingress/nginx.conf` is a **template** loaded into
  `/etc/nginx/templates/default.conf.template`. The official Nginx image
  substitutes environment variables (such as `INGRESS_API_URL`) and writes it
  into `conf.d/`, which is already inside the `http {}` block. That is why the
  file contains only `server {}` blocks.
- For ARM64 hosts set `COMPUTE_PLATFORM=linux/arm64` (in `.env` or inline). The
  Dockerfile itself is the same.

### 3.2 Microservices

Auth and to-do run as separate processes behind a frontend Nginx that serves the
static files and routes API paths. Build the shared image first, then combine the
three fragments:

```sh
docker build --platform linux/amd64 -t compute-unit .
docker compose --env-file .env \
  -f compose.yaml \
  -f nanoservices/auth/build-config.yml \
  -f nanoservices/to_do/build-config.yml \
  -f frontend/build-config.yml up --build
```

Then open <http://localhost>.

How it is wired:

- `frontend/nginx.conf` sends `/api/v1/auth/*` and `/api/v1/users/*` to the auth
  service, everything else under `/api/v1/` to the to-do service, and serves the
  built frontend for all other paths (single-page-app fallback). The upstream
  URLs come from `.env` (`AUTH_DOCKER_API_URL`, `TODO_DOCKER_API_URL`) and are
  substituted when the container starts.
- The to-do service resolves users through the auth service over HTTP
  (`auth-http` feature) and checks sessions in Redis.
- Auth and to-do are published on `127.0.0.1:8081` and `127.0.0.1:8080` only.

Differences from the monolith: there is no ingress, so there is no per-IP
token-bucket rate limiter, no Content-Security-Policy and no request timeout.
The frontend Nginx applies a basic `limit_req` to the auth and users paths, but
this layout is intended for learning and local use, not for production.

---

## 4. AWS production deployment

### 4.1 Overview of the steps

1. [Prepare AWS](#42-one-time-aws-preparation) (credentials, key pair).
2. [Request the ACM certificate](#43-domain-route-53-and-acm-certificate) and wait until it is `ISSUED`.
3. [Build and publish the images](#44-build-and-publish-the-images).
4. [Fill in `deployment/.env.production`](#45-configure-deploymentenvproduction).
5. (Optional) [Move state to S3](#46-state-backend-local-by-default-s3-optional).
6. [Run the deployment](#47-run-the-deployment).
7. [Point DNS at the load balancer](#48-point-route-53-dns-at-the-alb) and verify.
8. [Create the first user](#49-create-the-first-user).

### 4.2 One-time AWS preparation

The production Terraform stack is opinionated: it uses the **default VPC** in
`ap-southeast-1`, looks up the current Amazon Linux 2023 x86_64 AMI through SSM,
creates one EC2 application server and one RDS PostgreSQL instance, and puts an
internet-facing ALB in the default VPC subnets. It does not create a custom VPC
or private-subnet architecture for you.

1. **Choose the Region first.** In the AWS console and CLI, select
   `ap-southeast-1` (Singapore) unless you deliberately override
   `AWS_REGION`. The key pair, ALB, EC2, RDS and ACM certificate must be in the
   Region expected by the stack. `builds/main.tf` currently hard-codes the same
   Region for the temporary build EC2.
2. **Configure an AWS identity.** Use IAM Identity Center/SSO or an IAM role/user
   with the permissions needed for EC2, VPC, ELBv2, RDS, Security Groups,
   Elastic IP, SSM parameter reads and (if used) Route 53. Avoid root account
   access keys. Confirm the active identity:

   ```sh
   aws sts get-caller-identity
   aws configure list
   ```
3. **Check the default VPC and subnets.** The ALB needs subnets in at least two
   different Availability Zones. AWS also recommends each ALB subnet have at
   least a `/27` CIDR and at least eight free IP addresses so the load balancer
   can scale and replace nodes. The Nginx config also trusts the default VPC CIDR
   when parsing the ALB's `X-Forwarded-For` header:

   ```sh
   VPC_ID="$(aws ec2 describe-vpcs --region ap-southeast-1 \
     --filters Name=is-default,Values=true --query 'Vpcs[0].VpcId' --output text)"
   aws ec2 describe-vpcs --region ap-southeast-1 \
     --vpc-ids "$VPC_ID" --query 'Vpcs[0].[VpcId,CidrBlock]' --output table
   aws ec2 describe-subnets --region ap-southeast-1 \
     --filters "Name=vpc-id,Values=$VPC_ID" \
     --query 'Subnets[*].[AvailabilityZone,SubnetId,CidrBlock,AvailableIpAddressCount,MapPublicIpOnLaunch]' \
     --output table
   ```

   Record the VPC CIDR (often `172.31.0.0/16`). If it differs, update
   `set_real_ip_from` in `deployment/nginx.conf` before deployment. The current
   data source selects **all subnets in the default VPC**, rather than explicitly
   selecting public ALB subnets and isolated DB subnets. It therefore assumes a
   vanilla default VPC; adding custom/private subnets to that VPC can change what
   the ALB and RDS subnet group receive. If the default VPC is missing, lacks two
   suitable AZ subnets, or has ALB subnets with insufficient address space, the
   current IaC will fail or the ALB may not have enough addresses to scale. See
   AWS's [ALB subnet requirements](https://docs.aws.amazon.com/elasticloadbalancing/latest/application/application-load-balancers.html).
4. **Create the EC2 key pair.** The default name is `rwp_remotebuild`; the same
   key is used to SSH to both the temporary build server and production EC2.
   Key pairs are Region-specific. Create it once and store the private key
   securely:

   ```sh
   mkdir -p ~/.ssh/keys
   aws ec2 create-key-pair --region ap-southeast-1 --key-name rwp_remotebuild \
     --query KeyMaterial --output text > ~/.ssh/keys/rwp_remotebuild.pem
   chmod 400 ~/.ssh/keys/rwp_remotebuild.pem
   ```

   If the key pair already exists, AWS will not return its private key again;
   use the original `.pem` file or create a new key pair and update
   `ssh_key_name`/`SSH_KEY_PATH` consistently.
5. **Know the SSH source IP.** The deployment script detects your current public
   IPv4 address from `checkip.amazonaws.com` and uses `<your-ip>/32` for TCP 22.
   If you use a VPN, corporate egress, or a changing network, pass
   `SSH_CIDR=x.x.x.x/32` in `deployment/.env.production`. Do not open SSH to
   `0.0.0.0/0`; the Terraform variable explicitly rejects that value.

#### Public IP, Elastic IP and ALB DNS are three different things

| Address | Attached to | Purpose in this project | Should Route 53 point to it? |
|---|---|---|---|
| Auto-assigned public IPv4 | EC2 at launch | Temporary public address; AWS can change it after stop/start | No |
| Elastic IP (EIP) | Production EC2 primary network interface | Stable SSH endpoint returned as `ec2_public_ip`; lets your SSH target remain stable | **No**, not for the app hostname |
| ALB DNS name | Application Load Balancer | Public application entry point; DNS resolves to ALB nodes and AWS can update those IPs | **Yes**, use a Route 53 Alias A record |

The Terraform stack associates an EIP with the production EC2 instance. That EIP
is mainly useful for SSH and operational access. The web server Security Group
still accepts application traffic only from the ALB Security Group, so reaching
the EIP directly on port 80 does not bypass the ALB. The application domain must
point to the **ALB**, not the EC2 EIP. ALB node IPs are not a stable set of
addresses; do not copy the output of `dig` into static A records. AWS documents
EIP as a static, allocatable IPv4 address and ALB as a DNS-based endpoint whose
DNS can change as the service scales ([EIP guide](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/elastic-ip-addresses-eip.html),
[ALB DNS name](https://docs.aws.amazon.com/elasticloadbalancing/latest/application/application-load-balancers.html)).

#### Security Group paths created by this repository

| Security Group | Inbound | Why |
|---|---|---|
| ALB SG | TCP 80 and 443 from `0.0.0.0/0` | Public HTTP redirect and HTTPS listener |
| EC2 web SG | TCP 80 from the ALB SG; TCP 22 from `ssh_cidr` | Only ALB reaches Nginx; only your current IP can SSH |
| RDS SG | TCP 5432 from the EC2 web SG | Only the application host reaches PostgreSQL |

The RDS resource sets `publicly_accessible = false`; it is not intended to be
connected to from your laptop over the public Internet. Use an SSH tunnel,
Session Manager, VPN or a temporary controlled bastion path for administration.
The current Terraform does use the default VPC and a subnet group built from its
subnets, so this is a compact learning stack rather than a fully segregated
network design. For a hardened production network, use public subnets for the
ALB, private application subnets for EC2, isolated private DB subnets for RDS,
and explicit route tables/NAT or VPC endpoints.

### 4.3 Domain, Route 53 and ACM certificate

Use a domain name you control, for example `example.com`, and choose the exact
application hostname, for example `todo.example.com`. `DOMAIN_NAME` must match
that hostname and an ACM certificate in the **same Region as the ALB**.

#### A. Create or delegate a public hosted zone

If the domain's DNS is already hosted in Route 53, use its existing **public
hosted zone**. Otherwise:

1. In Route 53 → Hosted zones → Create hosted zone, enter the registered domain
   (for example `example.com`) and choose **Public hosted zone**.
2. Route 53 creates an NS record set. At the domain registrar, replace the
   domain's authoritative nameservers with the four Route 53 nameservers.
   Creating a hosted zone alone does not move DNS hosting; the registrar's NS
   delegation is what makes resolvers ask Route 53.
3. Verify delegation after propagation:

   ```sh
   dig NS example.com +short
   ```

   The result should match the NS values shown by the Route 53 hosted zone.

If you keep DNS at another provider, you do not need to delegate to Route 53;
add the ACM validation CNAME and later an application CNAME there instead. A
CNAME is suitable for a subdomain such as `todo.example.com`, not the zone apex
`example.com`.

#### B. Request and validate the public ACM certificate

Request a public certificate in `ap-southeast-1` for the exact app hostname. In
the console: Certificate Manager → Request → Public certificate → enter
`todo.example.com` → DNS validation. Or use the CLI:

```sh
aws acm request-certificate --region ap-southeast-1 \
  --domain-name todo.example.com --validation-method DNS
```

Get the certificate ARN from the command output, then inspect the DNS validation
record ACM expects:

```sh
aws acm describe-certificate --region ap-southeast-1 \
  --certificate-arn <certificate-arn> \
  --query 'Certificate.DomainValidationOptions[*].[DomainName,ResourceRecord.Name,ResourceRecord.Type,ResourceRecord.Value,ValidationStatus]' \
  --output table
```

- With Route 53 in the same account, open the certificate's domain validation
  details and choose **Create records in Route 53** when available.
- With an external DNS provider, create the exact CNAME name/value ACM returns.
  Do not append the domain twice; some DNS dashboards append the zone name
  automatically.
- Keep the validation CNAME in DNS after issuance. ACM uses it for managed
  renewal.

Wait for `ISSUED`:

```sh
aws acm list-certificates --region ap-southeast-1 \
  --query "CertificateSummaryList[?DomainName=='todo.example.com'].[CertificateArn,Status]" \
  --output table
```

`deployment/load_balancer.tf` looks up the most recent `ISSUED` certificate by
`DOMAIN_NAME`. If the domain, Region or status does not match, OpenTofu planning
fails with a missing-certificate error. AWS reference: [ACM DNS validation](https://docs.aws.amazon.com/acm/latest/userguide/dns-validation.html).

### 4.4 Build and publish the images

Before running the script, prepare Docker Hub:

1. Sign in to Docker Hub and verify the account email.
2. Create two repositories under the account that will own the images:
   `compute-unit` and `redis-cache`. The script uses these exact repository
   names; they are not configurable independently.
3. Choose **Public** or **Private** visibility deliberately. Public images can be
   pulled without authentication, but anyone can inspect/pull the published
   binaries and embedded frontend. Private repositories require a read-capable
   token during production deployment.
4. Create a Docker Hub Personal Access Token with repository **Read & Write**
   access for publishing. Store it in a password manager. For organization-owned
   automation, an Organization Access Token may be a better ownership model.

`builds/publish_images.sh` provisions a build server (if none exists), sends the
source (without `.env`, keys, state files or build output), builds both images
there with BuildKit, and pushes them to Docker Hub. It always performs
`docker login` on the build server, so a token is required even when the repos
are public:

```sh
cd builds
DOCKERHUB_USER=yourname ./publish_images.sh
```

When `DOCKERHUB_TOKEN` is not set, the script prompts for it without echoing
input. Prefer the prompt over writing a token in the command line, shell history,
or a checked-in script.

| Variable | Meaning |
|---|---|
| `DOCKERHUB_USER` (required) | 4–30 lowercase letters/digits |
| `DOCKERHUB_TOKEN` | Required by the build script; prompted for if unset. Sent to the server over SSH stdin, then supplied to `docker login --password-stdin` |
| `IMAGE_TAG` | Tag to push (default: git short SHA, `-dirty` if uncommitted, or a timestamp outside git). `latest` is pushed too |
| `SSH_CIDR` | CIDR allowed to SSH to the build server (default: your public IPv4 `/32`) |
| `SSH_KEY_PATH` | Private key (default `~/.ssh/keys/rwp_remotebuild.pem`) |
| `TF_BIN` | `terraform` instead of `tofu` |

The script prints the image names and the tag at the end. It creates:

- `<user>/compute-unit:<tag>`: ingress with the embedded frontend (plus the two
  standalone servers);
- `<user>/redis-cache:<tag>`: Redis 7.4 with the session module.

Things to know:

- The **build server keeps running (and billing) after the build**, which also
  keeps the Docker layer and Cargo caches warm for the next build. Destroy it
  with `./scripts/deployment_cleanup.sh --tofu` when you are done.
- Secrets are not baked into images (`.dockerignore` excludes `.env*`, keys and
  Terraform state); they are supplied at runtime. If your Docker Hub
  repositories are public, the binaries and frontend bundle are public. If they
  are private, set `DOCKERHUB_TOKEN` in `deployment/.env.production` with at
  least pull/read permission so production EC2 can log in before pulling.
- The build script logs out on its exit trap. The production deployment script
  currently runs `sudo docker login` for private repositories but does not run
  `docker logout` afterwards. Docker may retain registry authentication data in
  root's Docker config on EC2. Treat the token as a production secret, scope it
  narrowly, and consider this a cleanup hardening item.
- State lives in `builds/terraform.tfstate`. Keep it until you have destroyed the
  build server.
- `builds/run_build.sh` is the older path that builds a binary instead of
  images; production uses images.

### 4.5 Configure `deployment/.env.production`

```sh
cp deployment/.env.production.example deployment/.env.production
```

The file is plain `KEY=value` lines (it is parsed, never `source`d, so it is not
executed as shell; `$HOME` and a leading `~/` are expanded). It is git-ignored.

| Key | Required | Rules / how to generate |
|---|---|---|
| `DOCKERHUB_USER` | yes | 4–30 lowercase letters/digits |
| `IMAGE_TAG` | no (`latest`) | The tag printed by `publish_images.sh`; prefer an explicit tag |
| `DOCKERHUB_TOKEN` | only for private repos | Used for `docker login` on the server |
| `JWT_SECRET` | yes | ≥ 32 characters from `A-Za-z0-9+/=_.~-`; `openssl rand -hex 32` |
| `REDIS_PASSWORD` | yes | ≥ 16 characters from `A-Za-z0-9_.~-`; `openssl rand -hex 24` |
| `DB_USERNAME` | yes | RDS master user; starts with a letter, letters/digits/`_`, ≤ 30 |
| `DB_PASSWORD` | yes | 12–128 characters from `A-Za-z0-9_.~-`; `openssl rand -hex 16` |
| `APP_DB_PASSWORD` | yes | Same rules; password of the least-privilege `todo_app` role the application uses |
| `DOMAIN_NAME` | yes | Hostname that has an `ISSUED` ACM certificate |
| `AWS_REGION`, `SSH_CIDR`, `SSH_KEY_PATH`, `TF_BIN` | no | Optional overrides |

Why two database accounts: the RDS **master** user (`DB_USERNAME`) is only used
by the deployment script to create the `auth` database and the `todo_app` role.
The application connects as `todo_app`, which owns only the `to_do` and `auth`
databases.

### 4.6 State backend (local by default; S3 optional)

`deployment/backend.tf` uses the local backend by default:

```hcl
terraform {
  backend "local" {
    path = "terraform.tfstate"
  }
}
```

The state is stored in `deployment/terraform.tfstate` on your machine and is
ignored by Git. Losing it can orphan the AWS resources, so back it up securely.

To move state to S3 instead:

1. Create a private S3 bucket with versioning enabled (and suitable access
   controls). Keep the bucket outside the resources managed by this state.
2. Replace the `backend "local"` block in `deployment/backend.tf` with:

   ```hcl
   terraform {
     backend "s3" {
       bucket       = "your-unique-tfstate-bucket"
       key          = "deployment/terraform.tfstate"
       region       = "ap-southeast-1"
       encrypt      = true
       use_lockfile = true
     }
   }
   ```

3. Run `cd deployment && tofu init -migrate-state` and confirm the state was
   copied before deleting or moving the local state file.
4. Keep the state-cleanup behavior in `scripts/deployment_cleanup.sh` aligned
   with the backend you selected; do not delete local state before migration
   has been verified.

The current `deployment/main.tf` already requires OpenTofu/Terraform `>= 1.10.0`.

### 4.7 Run the deployment

```sh
./deployment/run_deployment.sh          # asks before applying
./deployment/run_deployment.sh --yes    # applies without asking
```

It is safe to re-run: it applies infrastructure changes, refreshes configuration,
pulls images and recreates only the containers that changed. Steps:

1. **Load and validate** `.env.production` (rejects weak or malformed values, and
   warns about unknown keys without echoing secrets).
2. **Detect your public IP** (`checkip.amazonaws.com`) and allow SSH only from it
   (override with `SSH_CIDR`).
3. **Plan and apply** with OpenTofu. A new RDS instance takes about 10 minutes.
   Secrets reach OpenTofu through `TF_VAR_*` environment variables, not arguments.
4. **Wait for SSH** and for the server's first-boot script (`server_build.sh`) to
   finish (Docker, the Compose plugin, log rotation).
5. **Write the server-side runtime `.env`** (mode 600): image names, JWT and
   Redis secrets, and the `todo_app` database URLs with the password
   percent-encoded and `?sslmode=require`. It does not contain the RDS master
   password.
6. **Upload** `compose.yaml`, `nginx.conf` and `init-databases.sql` to `~/app`.
   A separate `.env.bootstrap` (mode 600) temporarily carries the master and
   application DB passwords for database initialization.
7. **Initialize the database** by running `init-databases.sql` with `psql` from a
   throwaway container: it creates the `auth` database and the `todo_app` role
   and grants the databases to that role. It retries while RDS becomes ready.
   The remote script removes `.env.bootstrap` after the bootstrap step (and
   installs a cleanup trap); it is not used by the running application.
8. **`docker compose pull && docker compose up -d`** using `compose.yaml`, then
   wait for the application to answer through the load balancer
   (`https://<alb-dns>/healthz`, certificate name checks skipped because you
   connect by ALB name).
9. Print the **DNS target** (the ALB hostname) and handy commands.

Ingress runs the `to_do` and `auth` migrations at startup.

If you ever change `server_build.sh` (the EC2 user data), the server is **not**
replaced automatically. To apply it to a fresh server on purpose:
`cd deployment && tofu apply -replace=aws_instance.production` (with the same
`TF_VAR_*` variables the script exports).

### 4.8 Point Route 53 DNS at the ALB

The current `deployment/load_balancer.tf` contains an optional Route 53 resource
block, but it is **commented out**. `run_deployment.sh` does not create DNS
records. After the deployment prints `ALB_DNS`, create the record yourself.

#### Preferred: Route 53 Alias A record

1. Open Route 53 → Hosted zones → select the **public** hosted zone for your
   domain (for example `example.com`).
2. Choose **Create record**.
3. Enter the record name `todo` for `todo.example.com` (leave it blank for the
   zone apex if that is what you intend to serve).
4. Choose record type **A**.
5. Turn **Alias** on.
6. For Route traffic to, choose **Alias to Application and Classic Load
   Balancer**, select Region `ap-southeast-1`, then select the ALB created by
   this stack (`ToDoApplicationLb`).
7. Keep simple routing and enable target health evaluation, then create the
   record.

The ALB currently declares `ip_address_type = "ipv4"`, so create an **A** Alias
record (not AAAA). Alias records can target the ALB DNS name without pinning its
underlying node IPs. See [Route 53 alias records](https://docs.aws.amazon.com/Route53/latest/DeveloperGuide/routing-to-elb-load-balancer.html).

#### Alternative: DNS hosted by another provider

For a subdomain, create a CNAME from `todo.example.com` to the ALB DNS name
printed by `run_deployment.sh`. If you need the zone apex, use Route 53 Alias or
your DNS provider's equivalent ALIAS/ANAME flattening feature; a normal CNAME at
the apex is not valid.

**Do not point the application domain to the EC2 Elastic IP.** The EIP is the
stable SSH/instance address. Public HTTP(S) traffic must go to the ALB so that
ACM TLS termination, the HTTP→HTTPS redirect, target health checks and the ALB
security boundary work as designed.

Verify DNS and the app:

```sh
dig +short todo.example.com
curl -I http://todo.example.com          # expect 301 to HTTPS
curl -fsS https://todo.example.com/healthz && echo OK
aws elbv2 describe-target-health --region ap-southeast-1 \
  --target-group-arn "$(aws elbv2 describe-target-groups --region ap-southeast-1 \
    --names ToDoAppLbTg --query 'TargetGroups[0].TargetGroupArn' --output text)"
```

The target should be `healthy`, and the certificate presented for
`https://todo.example.com` must match that hostname. Do not use the ALB's own
`*.elb.amazonaws.com` hostname as the production HTTPS hostname: the public ACM
certificate is for `DOMAIN_NAME`, not the AWS-generated ALB hostname.

### 4.9 Create the first user

First set `INGRESS_URL=https://todo.example.com` in the **repository-root**
`.env` file. (The helper sources that file, so a command-line prefix for the same
variable is overwritten when `.env` already defines it.) Then run:

```sh
./scripts/create_user.sh you@example.com
```

Registration is limited to three attempts per hour per client IP, so avoid
repeated failed attempts. Then sign in at `https://todo.example.com`.

---

## 5. HTTP and HTTPS: how traffic is secured

| Hop | Protocol | Notes |
|---|---|---|
| Browser → ALB | HTTPS (TLS 1.2/1.3, policy `ELBSecurityPolicy-TLS13-1-2-2021-06`) | Certificate from ACM, renewed automatically |
| Browser → ALB on port 80 | HTTP | Answered with a `301` redirect to HTTPS by the ALB listener |
| ALB → Nginx | HTTP, port 80 | Inside the VPC. The web server's security group accepts port 80 only from the ALB's security group, so nothing can bypass the ALB |
| Nginx → ingress | HTTP, container network | Never leaves the Docker network |
| Ingress → Redis | Redis protocol, container network, password | Redis has no published port |
| Ingress → RDS | PostgreSQL over TLS | `sslmode=require` in the URL and `rds.force_ssl=1` on the database. SQLx is built with `tls-rustls` |

**TLS boundary:** TLS terminates at the ALB. The ALB-to-Nginx hop and the
Nginx-to-ingress hop are HTTP inside the VPC/Docker network, not end-to-end TLS.
The current protection relies on Security Groups and network isolation for
those internal hops. A stricter threat model would add authenticated/encrypted
service-to-service transport or a service mesh.

Related protections:

- **HSTS**: Nginx adds `Strict-Transport-Security` so browsers stay on HTTPS.
- **Forwarded protocol**: Nginx passes the ALB's `X-Forwarded-Proto` to ingress.
- **Client IP**: the ALB appends the client address to `X-Forwarded-For`. Nginx
  trusts only the VPC range, takes the real client IP, and gives ingress a clean
  `X-Real-IP`. Ingress, running with `TRUST_PROXY=1`, uses it for rate limiting
  (see [6.1](#61-production-nginx-deploymentnginxconf)).
- **Credentials**: login sends the password as Basic auth. HTTPS protects the
  browser-to-ALB hop. After TLS terminates at the ALB, the internal ALB-to-Nginx
  and Nginx-to-ingress hops are HTTP and rely on the VPC/Docker network boundary
  and Security Groups rather than end-to-end encryption.

### Alternatives (not implemented in this repository)

- **No load balancer:** terminate TLS on the EC2 instance itself (for example
  Nginx with certbot/Let's Encrypt, or Caddy). You would open port 443 on the
  instance, manage certificate renewal, and set `TRUST_PROXY` according to how
  the client IP reaches ingress. It is cheaper but you operate the certificate.
- **CloudFront in front of the ALB:** adds caching and a WAF, at the cost of
  one more moving part.

---

## 6. Nginx configurations explained

The repository has three Nginx configurations for three jobs.

| File | Where it runs | Loaded as | Role |
|---|---|---|---|
| `deployment/nginx.conf` | Production EC2 | Complete `/etc/nginx/nginx.conf` | Reverse proxy behind the ALB |
| `ingress/nginx.conf` | Local monolith | Template → `conf.d/default.conf` | TLS termination with a self-signed certificate |
| `frontend/nginx.conf` | Local microservices | Template → `conf.d/default.conf` | Static files and API routing |

A **complete** `nginx.conf` contains `events {}` and `http {}`. A **template** in
`/etc/nginx/templates/` is placed inside an existing `http {}` block, so it holds
only `server {}` (and `http`-level directives such as `map` and `limit_req_zone`).
Mixing the two up makes Nginx fail to start with
`"events" directive is not allowed here`.

### 6.1 Production Nginx (`deployment/nginx.conf`)

```nginx
set_real_ip_from  172.31.0.0/16;     # trust the ALB (the VPC range)
real_ip_header    X-Forwarded-For;
real_ip_recursive on;
```

The ALB sets `X-Forwarded-For` to *whatever the client sent* followed by the
client's real address. With `real_ip_recursive on`, Nginx walks the list from the
right, skips addresses in the trusted range and stops at the first untrusted one:
the real client. Anything a client wrote on the left is ignored. After this,
`$remote_addr` is the real client IP.

```nginx
proxy_set_header X-Real-IP        $remote_addr;
proxy_set_header X-Forwarded-For  $remote_addr;
proxy_set_header X-Forwarded-Proto $http_x_forwarded_proto;
```

Nginx **overwrites** `X-Real-IP` and `X-Forwarded-For` with the clean value, so a
client cannot inject its own. This is what makes `TRUST_PROXY=1` safe in ingress,
and it is only safe because the instance's security group lets nothing but the ALB
reach Nginx.

```nginx
map $uri $auth_limit_key { default ""; ~^/api/v1/(auth|users)/ $binary_remote_addr; }
limit_req_zone $auth_limit_key zone=auth:10m rate=30r/m;
limit_req zone=auth burst=10 nodelay;
```

A cheap first line of defence for login and registration: 30 requests per minute
per IP (burst 10). Requests to other paths get an empty key and are not limited.
Ingress's own token buckets still apply on top.

```nginx
resolver 127.0.0.11 valid=10s ipv6=off;
set $upstream http://ingress:8001;
proxy_pass $upstream;
```

Using a variable in `proxy_pass` makes Nginx resolve `ingress` at request time
through Docker's embedded DNS, so recreating the ingress container (and giving it
a new IP) does not require restarting Nginx.

The server block also sends `Strict-Transport-Security`,
`X-Content-Type-Options: nosniff` and `Referrer-Policy`. Compression is done by
ingress (not Nginx) so responses are not compressed twice.

**Adjust `set_real_ip_from`** if your VPC CIDR is not `172.31.0.0/16`
([4.2](#42-one-time-aws-preparation)). If it is wrong, every request appears to
come from the ALB and all users share one rate-limit bucket.

### 6.2 Local TLS Nginx (`ingress/nginx.conf`)

Redirects port 80 to 443 and proxies `/` to `${INGRESS_API_URL}` (substituted
from the environment by the image's template mechanism). It sets `Host`,
`X-Real-IP` and `X-Forwarded-Proto` so ingress sees the right client and scheme.
The certificate and key come from `ingress/self.crt` and `ingress/self.key`
(`./scripts/gen_dev_cert.sh`).

### 6.3 Frontend Nginx (`frontend/nginx.conf`)

Routes by path prefix: `/api/v1/auth/` and `/api/v1/users/` to the auth service,
other `/api/v1/` paths to the to-do service, everything else to static files with
`try_files ... /index.html` so client-side routes work on refresh. Only
`AUTH_API_URL` and `TODO_API_URL` are substituted (`NGINX_ENVSUBST_FILTER`), so
Nginx's own `$variables` are left alone.

---

## 7. Operations

### Deploy a new version

```sh
cd builds
DOCKERHUB_USER=yourname IMAGE_TAG=v2 ./publish_images.sh
cd ..
# Set IMAGE_TAG=v2 in deployment/.env.production
./deployment/run_deployment.sh
```

Only containers whose image or configuration changed are recreated. **Publish the
new images before applying infrastructure changes that need them** (for example
enabling `rds.force_ssl` requires an app image built with SQLx TLS support).

### Roll back

Set `IMAGE_TAG` back to the previous tag and run `run_deployment.sh` again.
Database migrations are forward-only; a rollback across a migration needs care.

### Look at the server

```sh
ssh -i ~/.ssh/keys/rwp_remotebuild.pem ec2-user@<elastic-ip>
cd app
sudo docker compose ps
sudo docker compose logs -f ingress
sudo docker compose logs --tail=100 nginx cache
```

Container logs are rotated (10 MB × 3 files per container).

### Reach the database

The database is private; connect from the server using the runtime URL (which
contains only the least-privilege `todo_app` credentials):

```sh
cd app && sudo docker run --rm -it --env-file .env postgres:18-alpine \
  sh -c 'psql "$TO_DO_DB_URL"'
```

The runtime `.env` intentionally does **not** contain the RDS master password
or the temporary bootstrap password. For administrative tasks, use the master
credentials from your local `deployment/.env.production` through a trusted
administrative session. Do not copy the master password into the persistent
runtime environment on EC2. `.env.bootstrap` is only a temporary file for the
initial database setup and is removed by the deployment script.

### Backups

RDS takes automated daily backups (retention **1 day by default**). The current
Terraform default is `db_protect = false`: deletion protection is off and a
destroy skips the final snapshot. Before storing data you care about, run the
deployment with `TF_VAR_db_protect=true` in the environment so the RDS instance
gets 7-day backups, deletion protection and a final snapshot on destroy. This
variable is not in `.env.production`; it must be exported in the shell that
invokes `run_deployment.sh` (for example `TF_VAR_db_protect=true ./deployment/run_deployment.sh`).

### Rotate secrets

The persistent runtime `.env` on EC2 contains only credentials required by the
application. Master DB credentials are kept in your local
`deployment/.env.production` and passed through a temporary `.env.bootstrap`
during deployment; the remote bootstrap file is removed after setup.

| Secret | How |
|---|---|
| `JWT_SECRET` | Change it and re-run `run_deployment.sh`. All issued tokens become invalid (everyone logs in again). |
| `REDIS_PASSWORD` | Change it and re-run; the cache and ingress containers are recreated, which also clears all sessions. |
| `APP_DB_PASSWORD` | Change it and re-run; the script updates the `todo_app` role and rewrites the runtime database URLs. |
| `DB_PASSWORD` | Changing it modifies the RDS master password (OpenTofu apply). Re-run `run_deployment.sh`; the new value is used only during bootstrap, not stored in runtime `.env`. |

### Restarting Redis logs everyone out

Sessions live only in Redis memory. Any restart or recreation of the `cache`
container invalidates all sessions; users simply sign in again.

### Scaling

The stack is a single web server on purpose. Running several app instances would
need a shared Redis (for example ElastiCache with the session module, which is not
available there) or moving sessions elsewhere. Vertical scaling (`instance_type`,
`db_instance_class`) is the easy path.

### CI

`.github/workflows/ci.yml` builds the Docker image and validates the Compose
files on every push, so broken deployment files are caught before you deploy.

---

## 8. Tear down

`scripts/deployment_cleanup.sh` removes things in increasing order of
destructiveness. By default it only stops local Compose containers and keeps
volumes and images.

| Option | Effect |
|---|---|
| *(none)* | Stop and remove Compose containers and networks (both local layouts) |
| `--volumes` | Also delete Docker volumes (**deletes local PostgreSQL data**) |
| `--images` | Also remove built images |
| `--tofu` | `tofu destroy` the **build server** (`builds/`) |
| `--prod` | `tofu destroy` the **production stack**: EC2, ALB, RDS and **all its data**. Never part of `--all` |
| `--local-files` | Delete `known_hosts`, `ingress.bin` and plan files (never the state) |
| `--all` | `--volumes --images --tofu --local-files` (not `--prod`) |
| `--dry-run` | Print the commands without running them |
| `--yes` | Skip the confirmation (otherwise type `CLEANUP`) |

Destroying production needs the domain (the certificate lookup happens during
planning) and, if you created it with protection, deletion protection turned off
first:

```sh
DOMAIN_NAME=todo.example.com ./scripts/deployment_cleanup.sh --prod --dry-run
DOMAIN_NAME=todo.example.com ./scripts/deployment_cleanup.sh --prod
```

If `db_protect=true`, run `tofu apply` with `-var db_protect=false` first (a
destroy cannot remove a protected database).

After destroying, check for leftovers that keep billing: unattached Elastic IPs,
the build server, RDS snapshots, and the S3 state bucket if you created one. The
ACM certificate itself is free. The script verifies that no tagged instances or
databases remain when the `aws` CLI is available.

---

## 9. Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| `no matching ACM Certificate found` during plan | The certificate is not `ISSUED`, is in another region, or does not match `DOMAIN_NAME` exactly ([4.3](#43-domain-route-53-and-acm-certificate)). |
| ALB target is `unhealthy` | Check `docker compose ps` on the server (cache must be healthy before ingress starts), `curl localhost/healthz` on the server, and that the web server security group allows port 80 from the ALB security group. |
| `502`/`504` from the ALB | Nginx or ingress is down or restarting: `docker compose logs nginx ingress`. |
| Everybody gets `429` on login | Client IP detection is broken: `TRUST_PROXY` is not `1` in the production Compose file, or `set_real_ip_from` does not match the VPC CIDR. |
| Ingress cannot connect to PostgreSQL / `SSL` errors | The URL needs `?sslmode=require` and the app image must be built with `tls-rustls`; an old image combined with `rds.force_ssl=1` fails. Publish the new image first. |
| `password authentication failed for user "todo_app"` | The role was not created or `APP_DB_PASSWORD` changed without re-running the script. Re-run `run_deployment.sh`. |
| `SSH not reachable after N attempts` | Your public IP changed or a VPN is in use. Re-run with `SSH_CIDR=<ip>/32`. |
| Nginx exits with `"events" directive is not allowed here` | A complete `nginx.conf` was mounted as a template (or vice versa) ([6](#6-nginx-configurations-explained)). |
| Invalid URL / connection errors in local `.env` | Passwords were generated with base64 (`/`, `+`, `=`). Regenerate them with `openssl rand -hex 32`. |
| `tofu destroy` fails on the database | Deletion protection is on ([8](#8-tear-down)). |
| `docker build` fails on `RUN --mount` | Ensure BuildKit is enabled and the Dockerfile starts with the valid `# syntax=docker/dockerfile:1.7` directive. Use `DOCKER_BUILDKIT=1 docker build ...` if needed (the publish script already does). |
| Changes to `server_build.sh` did nothing | User data changes do not replace the server; use `tofu apply -replace=aws_instance.production` if you really want a fresh server. |

---

## 10. Pre-launch checklist

- [ ] `deployment/.env.production` filled in with freshly generated hex secrets, and never committed.
- [ ] ACM certificate `ISSUED` in the same region as the ALB; DNS points at the ALB.
- [ ] `http://` redirects to `https://`; `https://<domain>/healthz` returns 200.
- [ ] Web server security group: port 80 from the ALB only; SSH from your IP only.
- [ ] `set_real_ip_from` matches the VPC CIDR; `TRUST_PROXY=1` only in the stack where ingress is reachable only through Nginx.
- [ ] A test login from two different networks does not share one rate-limit bucket.
- [ ] `IMAGE_TAG` is an explicit tag, not `latest`.
- [ ] Decide on `db_protect` (backups, deletion protection) before storing data you care about.
- [ ] The build server is destroyed when you are not building (`--tofu`).
- [ ] `deployment/backend.tf` matches the intended backend; local state is backed up or S3 state is versioned, and `.terraform.lock.hcl` files are committed.
- [ ] Runtime `.env` on EC2 contains only application credentials; `.env.bootstrap` has been removed after database initialization.
- [ ] The deployment uploaded `compose.yaml` (not `docker-compose.yml`) and the current image tag.
