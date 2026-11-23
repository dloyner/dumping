#!/bin/bash
# EC2 user_data: bootstrap a build server on Amazon Linux 2023.
# Runs once as root at first boot. Logs: /var/log/user-data.log

set -euxo pipefail
exec > >(tee -a /var/log/user-data.log) 2>&1

# --- System packages -----
dnf upgrade -y
dnf install -y git cmake tree vim-enhanced tmux nodejs openssl-devel
dnf groupinstall -y "Development Tools"
dnf install -y libpq-devel

# --- Docker -----
dnf install -y docker
systemctl enable --now docker 
usermod -aG docker ec2-user

mkdir -p /usr/local/lib/docker/cli-plugins
curl -fsSL \
  "https://github.com/docker/compose/releases/latest/download/docker-compose-linux-$(uname -m)" \
  -o /usr/local/lib/docker/cli-plugins/docker-compose
chmod +x /usr/local/lib/docker/cli-plugins/docker-compose

# --- Done: marker file polled by run_build.sh --------------------------------
echo "FINISHED" > /home/ec2-user/output.txt
chown ec2-user:ec2-user /home/ec2-user/output.txt

