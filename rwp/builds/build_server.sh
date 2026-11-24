#!/bin/bash
# EC2 user_data: bootstrap a build server on Amazon Linux 2023.
# Runs once as root at first boot. Logs: /var/log/user-data.log

set -euxo pipefail
exec > >(tee -a /var/log/user-data.log) 2>&1

# --- System packages -----
dnf upgrade -y
dnf install -y git cmake tree vim-enhanced tmux openssl-devel
dnf groupinstall -y "Development Tools"
dnf install -y libpq-devel unzip

# --- Bun (replaces Node.js/npm) ----------------------------------------------
BUN_VERSION="1.4.4"
curl -fsSL https://bun.sh/install | BUN_INSTALL=/usr/local bash -s -- ${BUN_VERSION:+bun-v$BUN_VERSION}
/usr/local/bin/bun --version

# --- Docker (used by publish_images.sh to build and push the production images) ---
dnf install -y docker
systemctl enable --now docker
usermod -aG docker ec2-user
docker --version

# --- Done: marker file polled by run_build.sh and publish_images.sh ------------
echo "FINISHED" > /home/ec2-user/output.txt
chown ec2-user:ec2-user /home/ec2-user/output.txt
