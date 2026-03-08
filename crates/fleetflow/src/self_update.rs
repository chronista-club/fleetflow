use colored::Colorize;

/// FleetFlow self-update: GitHub Releasesから最新版をダウンロードして更新
pub async fn self_update() -> anyhow::Result<()> {
    use std::process::Command;

    println!("{}", "FleetFlow self-update".blue().bold());
    println!();

    let current_version = env!("CARGO_PKG_VERSION");
    println!("現在のバージョン: {}", current_version.cyan());

    // GitHub APIから最新リリース情報を取得
    println!("最新バージョンを確認中...");

    let client = reqwest::Client::new();
    let response = client
        .get("https://api.github.com/repos/chronista-club/fleetflow/releases/latest")
        .header("User-Agent", "fleetflow")
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "GitHubからリリース情報を取得できませんでした: {}",
            response.status()
        ));
    }

    let release: serde_json::Value = response.json().await?;
    let latest_version = release["tag_name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("tag_nameが見つかりません"))?
        .trim_start_matches('v');

    println!("最新バージョン: {}", latest_version.green());

    // バージョン比較
    if !is_newer_version(latest_version, current_version) {
        println!();
        println!("{}", "✓ 既に最新版です！".green().bold());
        return Ok(());
    }

    println!();
    println!(
        "{}",
        format!("{} → {} に更新します", current_version, latest_version).yellow()
    );

    // ダウンロードURL決定
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let asset_name = match (os, arch) {
        ("macos", "aarch64") => "fleetflow-darwin-arm64.tar.gz",
        ("macos", "x86_64") => "fleetflow-darwin-amd64.tar.gz",
        ("linux", "x86_64") => "fleetflow-linux-amd64.tar.gz",
        ("linux", "aarch64") => "fleetflow-linux-arm64.tar.gz",
        _ => {
            return Err(anyhow::anyhow!(
                "このプラットフォームはサポートされていません: {}-{}",
                os,
                arch
            ));
        }
    };

    // ダウンロードURLを取得
    let assets = release["assets"].as_array();

    let download_url = assets.and_then(|arr| {
        arr.iter()
            .find(|a| a["name"].as_str() == Some(asset_name))
            .and_then(|a| a["browser_download_url"].as_str())
    });

    // バイナリがない場合は cargo install を使用
    let download_url = match download_url {
        Some(url) => url.to_string(),
        None => {
            println!(
                "{}",
                format!("プリビルドバイナリが見つかりません（{}）", asset_name).yellow()
            );
            println!("cargo install でビルドします...");
            println!();

            return cargo_install_update().await;
        }
    };

    println!("ダウンロード中: {}", asset_name);

    // 一時ディレクトリにダウンロード
    let temp_dir = std::env::temp_dir().join("fleetflow-update");
    std::fs::create_dir_all(&temp_dir)?;

    let tar_path = temp_dir.join(asset_name);

    // ダウンロード
    let response = client.get(&download_url).send().await?;
    let bytes = response.bytes().await?;
    std::fs::write(&tar_path, &bytes)?;

    println!("展開中...");

    // tar.gzを展開
    let output = Command::new("tar")
        .args([
            "-xzf",
            tar_path.to_str().unwrap(),
            "-C",
            temp_dir.to_str().unwrap(),
        ])
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "展開に失敗しました: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // 現在のバイナリパスを取得して直接置換
    let current_exe = std::env::current_exe()?;
    let new_binary = temp_dir.join("fleet");

    println!("インストール中: {}", current_exe.display());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&new_binary)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&new_binary, perms)?;
    }

    // 実行中バイナリを削除→コピーで置換
    if current_exe.exists()
        && let Err(e) = std::fs::remove_file(&current_exe)
    {
        println!();
        println!("{}", "⚠ バイナリの更新に失敗しました。".yellow());
        println!("権限が不足している可能性があります。以下のコマンドを実行してください:");
        println!();
        println!(
            "  sudo cp {} {}",
            new_binary.display(),
            current_exe.display()
        );
        println!();
        return Err(e.into());
    }

    match std::fs::copy(&new_binary, &current_exe) {
        Ok(_) => {
            println!();
            println!(
                "{}",
                format!("✓ FleetFlow {} に更新しました！", latest_version)
                    .green()
                    .bold()
            );
        }
        Err(e) => {
            println!();
            println!("{}", "⚠ バイナリのコピーに失敗しました。".yellow());
            println!(
                "  sudo cp {} {}",
                new_binary.display(),
                current_exe.display()
            );
            return Err(e.into());
        }
    }

    // クリーンアップ
    std::fs::remove_dir_all(&temp_dir).ok();

    Ok(())
}

/// バージョン比較: new_ver が current_ver より新しければ true
fn is_newer_version(new_ver: &str, current_ver: &str) -> bool {
    let parse_version =
        |v: &str| -> Vec<u32> { v.split('.').filter_map(|s| s.parse().ok()).collect() };

    let new_parts = parse_version(new_ver);
    let current_parts = parse_version(current_ver);

    for (n, c) in new_parts.iter().zip(current_parts.iter()) {
        if n > c {
            return true;
        }
        if n < c {
            return false;
        }
    }

    new_parts.len() > current_parts.len()
}

/// cargo install でFleetFlowを更新
async fn cargo_install_update() -> anyhow::Result<()> {
    use std::process::Command;

    println!(
        "{}",
        "cargo install --git https://github.com/chronista-club/fleetflow --force".cyan()
    );
    println!();

    let status = Command::new("cargo")
        .args([
            "install",
            "--git",
            "https://github.com/chronista-club/fleetflow",
            "--force",
        ])
        .status()?;

    if status.success() {
        println!();
        println!("{}", "✓ FleetFlow を更新しました！".green().bold());
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "cargo install に失敗しました（終了コード: {:?}）",
            status.code()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version_major() {
        assert!(is_newer_version("1.0.0", "0.9.0"));
        assert!(!is_newer_version("0.9.0", "1.0.0"));
    }

    #[test]
    fn test_is_newer_version_minor() {
        assert!(is_newer_version("0.10.0", "0.9.0"));
        assert!(!is_newer_version("0.9.0", "0.10.0"));
    }

    #[test]
    fn test_is_newer_version_patch() {
        assert!(is_newer_version("0.9.1", "0.9.0"));
        assert!(!is_newer_version("0.9.0", "0.9.1"));
    }

    #[test]
    fn test_is_newer_version_equal() {
        assert!(!is_newer_version("0.9.0", "0.9.0"));
    }

    #[test]
    fn test_is_newer_version_extra_digits() {
        assert!(is_newer_version("0.9.0.1", "0.9.0"));
        assert!(!is_newer_version("0.9.0", "0.9.0.1"));
    }
}
