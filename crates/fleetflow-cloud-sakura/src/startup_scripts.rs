//! Built-in startup scripts for FleetFlow
//!
//! These scripts are automatically created in Sakura Cloud when referenced
//! by `startup_script` in the server configuration.

/// mise installer script
/// Installs mise (task runner + tool version manager)
//
// raw string delimiter は `r##"..."##` を使用。
// 内部 bash の `"# FLEETFLOW: ..."` (= double-quote 内に # を持つ comment string) が
// `r#"..."#` の終端 `"#` と衝突するため delimiter を 1 段増やしている。
pub const MISE_SETUP: &str = r##"#!/bin/bash
# @sacloud-name "fleetflow-mise-setup"
# @sacloud-once
# @sacloud-desc FleetFlow: mise (タスクランナー + ツールバージョン管理) をインストール

set -e

echo "=== FleetFlow: mise セットアップ ==="

# 非対話 ssh / login shell の両経路で PATH を通すヘルパ。
# Debian/Ubuntu の .bashrc は冒頭に `case $- in *i*) ;; *) return;; esac` を持つため、
# 末尾 append された eval は `ssh host 'cmd'` 経由で実行されない (PATH が通らない)。
# return ガードの直前に marker 付きブロックを idempotent 挿入する。
write_noninteractive_path() {
    local target="$1"
    [ -f "$target" ] || return 0
    # CWE-59 (symlink-following): user-controlled home 配下 target が symlink だと
    # root の write が任意 file に到達する。 symlink は明示拒否。
    if [ -L "$target" ]; then
        echo "  skip: $target is a symlink (refusing to follow)" >&2
        return 0
    fi
    local begin="# FLEETFLOW: non-interactive PATH (auto-managed, do not edit)"
    local end="# FLEETFLOW: end non-interactive PATH"
    if grep -qF "$begin" "$target"; then
        return 0
    fi
    # 元 ownership / mode を保全 (= /home/<user>/.bashrc を root 所有化しない)
    local owner_uid owner_gid mode
    owner_uid=$(stat -c '%u' "$target")
    owner_gid=$(stat -c '%g' "$target")
    mode=$(stat -c '%a' "$target")
    # stage は root-only directory (= /root, mode 700)。 unprivileged user が
    # 干渉できる場所 (user home / /tmp) には作らない。
    local tmp
    tmp=$(mktemp -p /root fleetflow-bashrc.XXXXXX) || return 1
    local block="${begin}
if [ -x /usr/local/bin/mise ]; then
  eval \"\$(/usr/local/bin/mise activate bash)\"
elif [ -x \"\$HOME/.local/bin/mise\" ]; then
  eval \"\$(\$HOME/.local/bin/mise activate bash)\"
fi
${end}
"
    if grep -q '^case \$- in' "$target"; then
        awk -v block="$block" '
          !done && /^case \$- in/ { printf "%s", block; done=1 }
          { print }
        ' "$target" > "$tmp"
    else
        { printf '%s\n' "$block"; cat "$target"; } > "$tmp"
    fi
    # install -m/-o/-g で mode + owner + group を atomic に復元
    install -m "$mode" -o "$owner_uid" -g "$owner_gid" "$tmp" "$target"
    rm -f "$tmp"
}

# Install mise
if ! command -v mise &> /dev/null; then
    echo ">>> mise をインストール中..."
    curl -fsSL https://mise.run | sh

    # /usr/local/bin にシンボリックリンク (全ユーザー共通参照、 helper の前提)
    if [ -f /root/.local/bin/mise ] && [ ! -f /usr/local/bin/mise ]; then
        ln -s /root/.local/bin/mise /usr/local/bin/mise
    fi
fi

# 非対話 ssh で素のワンライナーが通るよう /etc/skel + 既存ユーザーに注入
write_noninteractive_path /etc/skel/.bashrc
write_noninteractive_path /root/.bashrc
[ -d /home/ubuntu ] && write_noninteractive_path /home/ubuntu/.bashrc

# login shell 経路の二重保険 (/etc/environment は PAM 経由のみで不十分)
PROFILE_TARGET=/etc/profile.d/fleetflow-path.sh
if [ -L "$PROFILE_TARGET" ]; then
    echo "  warning: $PROFILE_TARGET is a symlink, refusing to overwrite" >&2
else
    cat > "$PROFILE_TARGET" << 'PROFILE_PATH'
# FLEETFLOW: login shell PATH (auto-managed, do not edit)
[ -x /usr/local/bin/mise ] && eval "$(/usr/local/bin/mise activate bash)"
PROFILE_PATH
    chmod 644 "$PROFILE_TARGET"
fi

echo "✅ mise インストール完了"
"##;

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

/// Worker init script
/// Sets hostname and connects Tailscale with an auth key.
/// Used as a startup script when creating workers from the base archive.
pub const WORKER_INIT: &str = r#"#!/bin/bash
# @sacloud-name "fleetflow-worker-init"
# @sacloud-once
# @sacloud-desc FleetFlow: Worker 初期化（hostname + Tailscale 接続）
# @sacloud-text shellvar "hostname" "ホスト名"
# @sacloud-text shellvar "tailscale_authkey" "Tailscale Auth Key"

set -e

echo "=== FleetFlow: Worker 初期化 ==="

# hostname 設定
if [ -n "$hostname" ]; then
    echo ">>> hostname を ${hostname} に設定..."
    hostnamectl set-hostname "$hostname"
    echo "127.0.0.1 ${hostname}" >> /etc/hosts
fi

# Tailscale 接続
if [ -n "$tailscale_authkey" ] && command -v tailscale &> /dev/null; then
    echo ">>> Tailscale に接続中..."
    tailscale up \
        --authkey="$tailscale_authkey" \
        --hostname="${hostname:-$(hostname)}" \
        --ssh
    echo "  Tailscale IP: $(tailscale ip -4 2>/dev/null || echo 'pending')"
fi

echo "✅ Worker 初期化完了"
"#;

/// Get the script content for a built-in script name
pub fn get_builtin_script(name: &str) -> Option<&'static str> {
    match name {
        "fleetflow-mise-setup" => Some(MISE_SETUP),
        "fleetflow-docker-setup" => Some(DOCKER_SETUP),
        "fleetflow-fleetflow-setup" => Some(FLEETFLOW_SETUP),
        "fleetflow-worker-init" => Some(WORKER_INIT),
        _ => None,
    }
}

/// Check if a script name is a built-in FleetFlow script
pub fn is_builtin_script(name: &str) -> bool {
    get_builtin_script(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_builtin_script_mise() {
        let script = get_builtin_script("fleetflow-mise-setup");
        assert!(script.is_some());
        let content = script.unwrap();
        assert!(content.starts_with("#!/bin/bash"));
        assert!(content.contains("mise"));
        assert!(content.contains("@sacloud-name"));
    }

    #[test]
    fn test_get_builtin_script_docker() {
        let script = get_builtin_script("fleetflow-docker-setup");
        assert!(script.is_some());
        let content = script.unwrap();
        assert!(content.starts_with("#!/bin/bash"));
        assert!(content.contains("docker"));
        assert!(content.contains("@sacloud-once"));
    }

    #[test]
    fn test_get_builtin_script_fleetflow() {
        let script = get_builtin_script("fleetflow-fleetflow-setup");
        assert!(script.is_some());
        let content = script.unwrap();
        assert!(content.starts_with("#!/bin/bash"));
        assert!(content.contains("fleetflow"));
        assert!(content.contains("FLEETFLOW_VERSION"));
    }

    #[test]
    fn test_get_builtin_script_unknown() {
        assert!(get_builtin_script("nonexistent-script").is_none());
        assert!(get_builtin_script("").is_none());
        assert!(get_builtin_script("fleetflow-").is_none());
    }

    #[test]
    fn test_get_builtin_script_worker_init() {
        let script = get_builtin_script("fleetflow-worker-init");
        assert!(script.is_some());
        let content = script.unwrap();
        assert!(content.starts_with("#!/bin/bash"));
        assert!(content.contains("tailscale"));
        assert!(content.contains("hostname"));
        assert!(content.contains("@sacloud-text shellvar"));
    }

    #[test]
    fn test_is_builtin_script_true() {
        assert!(is_builtin_script("fleetflow-mise-setup"));
        assert!(is_builtin_script("fleetflow-docker-setup"));
        assert!(is_builtin_script("fleetflow-fleetflow-setup"));
        assert!(is_builtin_script("fleetflow-worker-init"));
    }

    #[test]
    fn test_is_builtin_script_false() {
        assert!(!is_builtin_script("custom-script"));
        assert!(!is_builtin_script(""));
        assert!(!is_builtin_script("fleetflow-unknown"));
    }

    #[test]
    fn test_scripts_have_sacloud_annotations() {
        let scripts = [
            ("fleetflow-mise-setup", MISE_SETUP),
            ("fleetflow-docker-setup", DOCKER_SETUP),
            ("fleetflow-fleetflow-setup", FLEETFLOW_SETUP),
            ("fleetflow-worker-init", WORKER_INIT),
        ];

        for (name, content) in &scripts {
            assert!(
                content.contains("@sacloud-name"),
                "{} should have @sacloud-name",
                name
            );
            assert!(
                content.contains("@sacloud-once"),
                "{} should have @sacloud-once",
                name
            );
            assert!(
                content.contains("@sacloud-desc"),
                "{} should have @sacloud-desc",
                name
            );
            assert!(
                content.contains("set -e"),
                "{} should have set -e for error handling",
                name
            );
        }
    }

    #[test]
    fn test_script_names_in_content_match() {
        // Verify the @sacloud-name in each script matches its lookup key
        assert!(MISE_SETUP.contains("@sacloud-name \"fleetflow-mise-setup\""));
        assert!(DOCKER_SETUP.contains("@sacloud-name \"fleetflow-docker-setup\""));
        assert!(FLEETFLOW_SETUP.contains("@sacloud-name \"fleetflow-fleetflow-setup\""));
        assert!(WORKER_INIT.contains("@sacloud-name \"fleetflow-worker-init\""));
    }
}
