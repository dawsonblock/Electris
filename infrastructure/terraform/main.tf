# ================================================
# ELECTRO - Terraform Infrastructure
# Deploys a single compute instance running the
# ELECTRO Docker image with persistent storage.
#
# Supports: AWS (default), with variables to
# customise region, instance size, and image tag.
# ================================================

terraform {
  required_version = ">= 1.5"

  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.0"
    }
  }

  # Remote state (uncomment and configure for production)
  # backend "s3" {
  #   bucket = "electro-tfstate"
  #   key    = "electro/terraform.tfstate"
  #   region = "us-east-1"
  # }
}

# ------------------------------------------------
# Provider
# ------------------------------------------------

provider "aws" {
  region = var.aws_region

  default_tags {
    tags = {
      Project     = "electro"
      Environment = var.environment
      ManagedBy   = "terraform"
    }
  }
}

# ------------------------------------------------
# Data sources
# ------------------------------------------------

data "aws_ami" "amazon_linux" {
  most_recent = true
  owners      = ["amazon"]

  filter {
    name   = "name"
    values = ["al2023-ami-*-x86_64"]
  }

  filter {
    name   = "virtualization-type"
    values = ["hvm"]
  }
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

# ------------------------------------------------
# Security Group
# ------------------------------------------------

resource "aws_security_group" "electro" {
  name_prefix = "electro-${var.environment}-"
  description = "ELECTRO gateway security group"
  vpc_id      = data.aws_vpc.default.id

  # Gateway HTTP port
  ingress {
    description = "ELECTRO Gateway"
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = var.allowed_cidrs
  }

  # SSH (optional, for debugging)
  dynamic "ingress" {
    for_each = var.enable_ssh ? [1] : []
    content {
      description = "SSH"
      from_port   = 22
      to_port     = 22
      protocol    = "tcp"
      cidr_blocks = var.allowed_cidrs
    }
  }

  egress {
    description = "All outbound"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  lifecycle {
    create_before_destroy = true
  }

  tags = {
    Name = "electro-${var.environment}"
  }
}

# ------------------------------------------------
# EBS Volume (persistent data)
# ------------------------------------------------

resource "aws_ebs_volume" "electro_data" {
  availability_zone = "${var.aws_region}a"
  size              = var.volume_size_gb
  type              = "gp3"
  encrypted         = true

  tags = {
    Name = "electro-data-${var.environment}"
  }
}

# ------------------------------------------------
# EC2 Instance
# ------------------------------------------------

resource "aws_instance" "electro" {
  ami                    = data.aws_ami.amazon_linux.id
  instance_type          = var.instance_type
  vpc_security_group_ids = [aws_security_group.electro.id]
  subnet_id              = data.aws_subnets.default.ids[0]
  availability_zone      = "${var.aws_region}a"

  key_name = var.ssh_key_name != "" ? var.ssh_key_name : null

  root_block_device {
    volume_size = 20
    volume_type = "gp3"
    encrypted   = true
  }

  user_data = <<-USERDATA
    #!/bin/bash
    set -euo pipefail

    # Install Docker
    yum update -y
    yum install -y docker
    systemctl enable docker
    systemctl start docker

    # Format and mount data volume
    DEVICE="/dev/xvdf"
    MOUNT="/var/lib/electro"
    if ! blkid "$DEVICE" > /dev/null 2>&1; then
      mkfs.ext4 "$DEVICE"
    fi
    mkdir -p "$MOUNT"
    mount "$DEVICE" "$MOUNT"
    echo "$DEVICE $MOUNT ext4 defaults,nofail 0 2" >> /etc/fstab

    # Create electro config directory
    mkdir -p "$MOUNT/.electro"

    # Pull and run ELECTRO
    docker pull ${var.docker_image}:${var.docker_tag}
    docker run -d \
      --name electro \
      --restart unless-stopped \
      -p 8080:8080 \
      -v "$MOUNT/.electro:/var/lib/electro" \
      -e ELECTRO_MODE=${var.electro_mode} \
      -e ANTHROPIC_API_KEY=${var.anthropic_api_key} \
      -e TELEGRAM_BOT_TOKEN=${var.telegram_bot_token} \
      -e RUST_LOG=${var.log_level} \
      ${var.docker_image}:${var.docker_tag}
  USERDATA

  tags = {
    Name = "electro-${var.environment}"
  }

  lifecycle {
    ignore_changes = [ami]
  }
}

# Attach the persistent data volume
resource "aws_volume_attachment" "electro_data" {
  device_name = "/dev/xvdf"
  volume_id   = aws_ebs_volume.electro_data.id
  instance_id = aws_instance.electro.id
}

# ------------------------------------------------
# Elastic IP (stable public address)
# ------------------------------------------------

resource "aws_eip" "electro" {
  count    = var.enable_eip ? 1 : 0
  instance = aws_instance.electro.id
  domain   = "vpc"

  tags = {
    Name = "electro-${var.environment}"
  }
}
