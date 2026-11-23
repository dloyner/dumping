terraform {
  required_version = ">= 1.9.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.62.0, < 7.0.0"
    }
  }
}

provider "aws" {
  region = "ap-southeast-1"
}

variable "ssh_cidr" {
  description = "CIDR allowed to SSH into the build server"
  type        = string

  validation {
    condition     = can(cidrhost(var.ssh_cidr, 0)) && var.ssh_cidr != "0.0.0.0/0"
    error_message = "ssh_cidr must be a valid CIDR block and must not be 0.0.0.0/0"
  }
}

resource "aws_security_group" "build_server" {
  name_prefix = "build-server-"
  description = "SSH access for the build server"

  ingress {
    description = "SSH from my IP only"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = [var.ssh_cidr]
  }

  egress {
    description = "Allow all outbound (package installs, git, docker pulls)"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "to-do build server"
  }

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_instance" "build_server" {
  ami           = "ami-026b70e9abd9fa20f"
  instance_type = "m7i-flex.large"
  key_name      = "rwp_remotebuild"

  vpc_security_group_ids      = [aws_security_group.build_server.id]
  associate_public_ip_address = true

  user_data                   = file("${path.module}/build_server.sh")
  user_data_replace_on_change = true

  root_block_device {
    volume_size = 30
  }

  metadata_options {
    http_tokens = "required"
  }

  tags = {
    Name = "to-do build server"
  }
}

output "ec2_public_ip" {
  description = "Public IPv4 address of the build server"
  value       = aws_instance.build_server.public_ip
}
