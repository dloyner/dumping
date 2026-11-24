#!/bin/bash
# deployment/server_build.sh
# EC2 user_data: prepare the production server (Amazon Linux 2023) for Docker Compose.
# Runs once as root at first boot. Logs: /var/log/user-data.log

set -euxo pipefail
exec > >(tee -a /var/log/user-data.log) 2>&1

COMPOSE_VERSION="v2.29.7" # bump deliberately; see https://github.com/docker/compose/releases

dnf upgrade -y
dnf install -y docker

# Keep container logs from filling the disk.
install -d /etc/docker
cat > /etc/docker/daemon.json <<'JSON'
{
  "log-driver": "json-file",
  "log-opts": { "max-size": "10m", "max-file": "3" }
}
JSON

systemctl enable --now docker
usermod -aG docker ec2-user

# Docker Compose v2 is not packaged for AL2023, so install the CLI plugin.
install -d /usr/local/lib/docker/cli-plugins
curl -fsSL "https://github.com/docker/compose/releases/download/${COMPOSE_VERSION}/docker-compose-linux-x86_64" \
  -o /usr/local/lib/docker/cli-plugins/docker-compose
chmod +x /usr/local/lib/docker/cli-plugins/docker-compose
docker compose version

# Application directory used by run_deployment.sh
install -d -m 700 -o ec2-user -g ec2-user /home/ec2-user/app

# Marker file polled by run_deployment.sh
echo "FINISHED" > /home/ec2-user/output.txt
chown ec2-user:ec2-user /home/ec2-user/output.txt
