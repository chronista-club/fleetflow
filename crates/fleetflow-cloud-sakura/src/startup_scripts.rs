//! Built-in startup scripts for FleetFlow
//!
//! These scripts are automatically created in Sakura Cloud when referenced
//! by `startup_script` in the server configuration.

/// mise installer script
/// Installs mise (task runner + tool version manager)
pub const MISE_SETUP: &str = r#"#!/bin/bash
# @sacloud-name "fleetflow-mise-setup"
# @sacloud-once
# @sacloud-desc FleetFlow: mise (タスクランナー + ツールバージョン管理) をインストール

set -e

echo "=== FleetFlow: mise セットアップ ==="

# Install mise
if ! command -v mise &> /dev/null; then
    echo ">>> mise をインストール中..."
    curl -fsSL https://mise.run | sh

    # Add to bashrc for all users
    echo 'eval "$($HOME/.local/bin/mise activate bash)"' >> /etc/skel/.bashrc

    # Add for root
    echo 'eval "$($HOME/.local/bin/mise activate bash)"' >> /root/.bashrc

    # Add for ubuntu user if exists
    if id "ubuntu" &>/dev/null; then
        sudo -u ubuntu bash -c 'echo "eval \"\$(\$HOME/.local/bin/mise activate bash)\"" >> ~/.bashrc'
    fi
fi

echo "✅ mise インストール完了"
"#;

/// Docker installer script
/// Installs Docker and Docker Compose
pub const DOCKER_SETUP: &str = r#"#!/bin/bash
# @sacloud-name "fleetflow-docker-setup"
# @sacloud-once
# @sacloud-desc FleetFlow: Docker と Docker Compose をインストール

set -e

echo "=== FleetFlow: Docker セットアップ ==="

# Install Docker if not present
if ! command -v docker &> /dev/null; then
    echo ">>> Docker をインストール中..."
    curl -fsSL https://get.docker.com | sh

    # Add ubuntu user to docker group
    if id "ubuntu" &>/dev/null; then
        usermod -aG docker ubuntu
    fi
fi

# Enable and start Docker
systemctl enable docker
systemctl start docker

echo "✅ Docker インストール完了"
"#;

/// FleetFlow installer script
/// Installs FleetFlow CLI
pub const FLEETFLOW_SETUP: &str = r#"#!/bin/bash
# @sacloud-name "fleetflow-fleetflow-setup"
# @sacloud-once
# @sacloud-desc FleetFlow: FleetFlow CLI をインストール

set -e

echo "=== FleetFlow: FleetFlow CLI セットアップ ==="

# Get latest version from GitHub
FLEETFLOW_VERSION=$(curl -s https://api.github.com/repos/osousa/fleetflow/releases/latest | grep tag_name | cut -d'"' -f4)

if [ -z "$FLEETFLOW_VERSION" ]; then
    echo "❌ FleetFlow バージョン取得に失敗"
    exit 1
fi

echo ">>> FleetFlow ${FLEETFLOW_VERSION} をインストール中..."

# Download and install
curl -L "https://github.com/osousa/fleetflow/releases/download/${FLEETFLOW_VERSION}/fleetflow-linux-amd64.tar.gz" -o /tmp/fleetflow.tar.gz
tar -xzf /tmp/fleetflow.tar.gz -C /tmp
mv /tmp/fleetflow /usr/local/bin/
chmod +x /usr/local/bin/fleetflow
rm /tmp/fleetflow.tar.gz

echo "✅ FleetFlow インストール完了"
"#;

/// Get the script content for a built-in script name
pub fn get_builtin_script(name: &str) -> Option<&'static str> {
    match name {
        "fleetflow-mise-setup" => Some(MISE_SETUP),
        "fleetflow-docker-setup" => Some(DOCKER_SETUP),
        "fleetflow-fleetflow-setup" => Some(FLEETFLOW_SETUP),
        _ => None,
    }
}

/// Check if a script name is a built-in FleetFlow script
pub fn is_builtin_script(name: &str) -> bool {
    get_builtin_script(name).is_some()
}
