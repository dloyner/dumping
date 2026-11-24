# deployment/main.tf
# Production stack: one EC2 instance (Docker Compose) + one private RDS PostgreSQL instance.

terraform {
  required_version = ">= 1.9.0"
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = ">= 5.62.0, < 6.0.0"
    }
  }
}

provider "aws" {
  region = var.aws_region
}

# --- Variables ---------------------------------------------------------------

variable "aws_region" {
  description = "AWS region for every resource"
  type        = string
  default     = "ap-southeast-1"
}

variable "ssh_cidr" {
  description = "CIDR allowed to SSH into the production server"
  type        = string

  validation {
    condition     = can(cidrnetmask(var.ssh_cidr)) && var.ssh_cidr != "0.0.0.0/0"
    error_message = "ssh_cidr must be a valid IPv4 CIDR block and must not be 0.0.0.0/0"
  }
}

variable "http_cidr" {
  description = "CIDR allowed to reach the web server on port 80"
  type        = string
  default     = "0.0.0.0/0"

  validation {
    condition     = can(cidrnetmask(var.http_cidr))
    error_message = "http_cidr must be a valid IPv4 CIDR block"
  }
}

variable "ssh_key_name" {
  description = "Name of the existing EC2 key pair in the configured region"
  type        = string
  default     = "rwp_remotebuild"
}

variable "instance_type" {
  description = "EC2 instance type for the production server (x86_64, matches the amd64 images)"
  type        = string
  default     = "t3.small"
}

variable "root_volume_gb" {
  description = "Root volume size in GiB"
  type        = number
  default     = 20
}

variable "db_instance_class" {
  description = "RDS instance class"
  type        = string
  default     = "db.t3.micro"
}

variable "db_engine_version" {
  description = "PostgreSQL major version on RDS (the parameter group family is derived from it)"
  type        = string
  default     = "17"
}

variable "db_username" {
  description = "Master username of the database"
  type        = string
  default     = "todo_admin"

  validation {
    condition     = can(regex("^[a-zA-Z][a-zA-Z0-9_]{0,29}$", var.db_username))
    error_message = "db_username must start with a letter and contain only letters, digits and underscores (max 30)"
  }
}

variable "db_password" {
  description = "Master password of the database (no default on purpose; pass it via TF_VAR_db_password)"
  type        = string
  sensitive   = true

  validation {
    condition     = can(regex("^[A-Za-z0-9_.~-]{12,128}$", var.db_password))
    error_message = "db_password must be 12-128 characters from A-Z a-z 0-9 _ . ~ - (URL-safe on purpose)"
  }
}

# --- Data sources ------------------------------------------------------------

# Current Amazon Linux 2023 x86_64 image in the configured region.
data "aws_ssm_parameter" "al2023_x86_64" {
  name = "/aws/service/ami-amazon-linux-latest/al2023-ami-kernel-default-x86_64"
}

data "aws_vpc" "default" {
  default = true
}

data "aws_subnets" "default" {
  filter {
    name   = "vpc-id"
    values = [data.aws_vpc.default.id]
  }
}

# --- Networking --------------------------------------------------------------

resource "aws_security_group" "production" {
  name_prefix = "todo-production-"
  description = "HTTP for everyone (var.http_cidr), SSH from var.ssh_cidr only"
  vpc_id      = data.aws_vpc.default.id

  ingress {
    description = "SSH from my IP only"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = [var.ssh_cidr]
  }

  ingress {
    description = "HTTP (nginx)"
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = [var.http_cidr]
  }

  egress {
    description = "Allow all outbound (Docker Hub pulls, package installs)"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "to-do production server"
  }

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_security_group" "database" {
  name_prefix = "todo-database-"
  description = "PostgreSQL reachable only from the production server"
  vpc_id      = data.aws_vpc.default.id

  ingress {
    description     = "PostgreSQL from the production server only"
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = [aws_security_group.production.id]
  }

  tags = {
    Name = "to-do production database"
  }

  lifecycle {
    create_before_destroy = true
  }
}

# --- Database ----------------------------------------------------------------

resource "aws_db_subnet_group" "main" {
  name_prefix = "todo-"
  description = "Default VPC subnets for the to-do database"
  subnet_ids  = data.aws_subnets.default.ids
}

# The workspace builds sqlx without a TLS feature, so connections must not be forced onto TLS.
# The database is private (no public access, reachable only from the production security group).
# To re-enable TLS: add the `tls-rustls` feature to sqlx in Cargo.toml and set this value to "1".
resource "aws_db_parameter_group" "main" {
  name_prefix = "todo-pg-"
  family      = "postgres${var.db_engine_version}"
  description = "to-do database: rds.force_ssl disabled because sqlx has no TLS support"

  parameter {
    name  = "rds.force_ssl"
    value = "0"
  }

  lifecycle {
    create_before_destroy = true
  }
}

resource "aws_db_instance" "main" {
  identifier_prefix = "todo-"
  engine            = "postgres"
  engine_version    = var.db_engine_version
  instance_class    = var.db_instance_class

  allocated_storage = 20
  storage_type      = "gp3"
  storage_encrypted = true

  # RDS creates only this database; deployment/init-databases.sql adds `auth`.
  db_name  = "to_do"
  username = var.db_username
  password = var.db_password

  db_subnet_group_name   = aws_db_subnet_group.main.name
  vpc_security_group_ids = [aws_security_group.database.id]
  parameter_group_name   = aws_db_parameter_group.main.name
  publicly_accessible    = false
  multi_az               = false

  backup_retention_period    = 1
  auto_minor_version_upgrade = true
  apply_immediately          = true

  # Learning setup: `destroy` deletes the data. Flip both for real production use.
  skip_final_snapshot = true
  deletion_protection = false

  tags = {
    Name = "to-do production database"
  }

  lifecycle {
    ignore_changes = [engine_version]
  }
}

# --- Application server ------------------------------------------------------

resource "aws_instance" "production" {
  ami           = data.aws_ssm_parameter.al2023_x86_64.value
  instance_type = var.instance_type
  key_name      = var.ssh_key_name

  vpc_security_group_ids      = [aws_security_group.production.id]
  associate_public_ip_address = true

  user_data                   = file("${path.module}/server_build.sh")
  user_data_replace_on_change = true

  root_block_device {
    volume_size = var.root_volume_gb
    volume_type = "gp3"
    encrypted   = true
  }

  metadata_options {
    http_tokens = "required"
  }

  tags = {
    Name = "to-do production server"
  }

  lifecycle {
    # A newer "latest" AMI must not silently replace a running production server.
    ignore_changes = [ami]
  }
}

# --- Outputs -----------------------------------------------------------------

output "ec2_public_ip" {
  description = "Public IPv4 address of the production server"
  value       = aws_instance.production.public_ip
}

output "db_address" {
  description = "Hostname of the RDS instance (no port)"
  value       = aws_db_instance.main.address
}
