#!/usr/bin/env bash
set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
openssl req -x509 -newkey rsa:2048 -nodes -days 365 \
  -keyout ingress/self.key -out ingress/self.crt \
  -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"
echo "Wrote ingress/self.crt and ingress/self.key"
