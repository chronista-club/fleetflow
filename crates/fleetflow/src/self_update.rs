use colored::Colorize;

/// FleetFlow self-update: GitHub Releasesから最新版をダウンロードして更新
pub async fn self_update() -> anyhow::Result<()> {
    use std::process::Command;

    println!("{}", "🔄 FleetFlow self-update".blue().bold());
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
    if current_version == latest_version {
        println!();
        println!("{}", "✓ 既に最新版です！".green().bold());
        return Ok(());
    }

    println!();
    println!(
        "{}",
        format!("新しいバージョン {} が利用可能です", latest_version).yellow()
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

    // 現在のバイナリパスを取得
    let current_exe = std::env::current_exe()?;
    let new_binary = temp_dir.join("fleet"); // バイナリ名は "fleet"

    // バイナリを置換
    println!("インストール中...");

    // まず古いバイナリをリネーム（バックアップ）
    let backup_path = current_exe.with_extension("old");
    if backup_path.exists() {
        std::fs::remove_file(&backup_path)?;
    }

    // 新しいバイナリをコピー
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // 実行権限を付与
        let mut perms = std::fs::metadata(&new_binary)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&new_binary, perms)?;
    }

    // Linuxでは実行中のバイナリでも「削除→コピー」で置換可能
    // （削除しても実行中プロセスはinode参照を保持するため動作継続）
    if current_exe.exists()
        && let Err(e) = std::fs::remove_file(&current_exe)
    {
        // 削除失敗時は権限不足の可能性
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

    // /usr/local/bin/fleet へのシンボリックリンクを作成
    ensure_usr_local_bin_symlink();

    // クリーンアップ（成功時のみ）
    std::fs::remove_dir_all(&temp_dir).ok();

    Ok(())
}

/// 起動時にバージョンチェックを行い、更新があれば通知・更新
/// CI/CD環境（CI環境変数が設定されている場合）ではスキップする
pub async fn check_and_update_if_needed() -> anyhow::Result<()> {
    // CI/CD環境では対話的プロンプトを出せないのでスキップ
    if std::env::var("CI").is_ok() || std::env::var("FLEETFLOW_NO_UPDATE_CHECK").is_ok() {
        return Ok(());
    }

    let current_version = env!("CARGO_PKG_VERSION");

    // GitHub APIから最新リリース情報を取得（タイムアウト短め）
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    let response = match client
        .get("https://api.github.com/repos/chronista-club/fleetflow/releases/latest")
        .header("User-Agent", "fleetflow")
        .send()
        .await
    {
        Ok(r) => r,
        Err(_) => {
            // ネットワークエラーは無視して続行
            return Ok(());
        }
    };

    if !response.status().is_success() {
        // APIエラーは無視して続行
        return Ok(());
    }

    let release: serde_json::Value = match response.json().await {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

    let latest_version = match release["tag_name"].as_str() {
        Some(tag) => tag.trim_start_matches('v'),
        None => return Ok(()),
    };

    // バージョン比較
    if is_newer_version(latest_version, current_version) {
        println!();
        println!(
            "📦 新しいバージョン {} が利用可能です（現在: {}）",
            latest_version.green(),
            current_version.yellow()
        );
        println!("{}", "   更新するには: fleet self-update".dimmed());
        println!();

        // 自動更新の確認
        print!("今すぐ更新しますか？ [y/N]: ");
        use std::io::Write;
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if input.trim().eq_ignore_ascii_case("y") {
            return self_update().await;
        }
        println!();
    }

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

    // 桁数が多い方が新しい (例: 1.0.1 > 1.0)
    new_parts.len() > current_parts.len()
}

/// /usr/local/bin/fleet へのシンボリックリンクを作成（~/.cargo/bin/fleet を指す）
fn ensure_usr_local_bin_symlink() {
    use std::os::unix::fs::symlink;
    use std::path::Path;

    let cargo_bin_fleet = dirs::home_dir()
        .map(|h| h.join(".cargo/bin/fleet"))
        .filter(|p| p.exists());

    let Some(cargo_bin) = cargo_bin_fleet else {
        // ~/.cargo/bin/fleet が存在しない場合はスキップ
        return;
    };

    let usr_local_bin = Path::new("/usr/local/bin/fleet");

    // 既にシンボリックリンクで正しいリンク先を指している場合はスキップ
    if usr_local_bin.is_symlink()
        && let Ok(target) = std::fs::read_link(usr_local_bin)
        && target == cargo_bin
    {
        println!(
            "{}",
            "✓ /usr/local/bin/fleet は既に正しくリンクされています".dimmed()
        );
        return;
    }

    // 既存のファイル/シンボリックリンクを削除してから作成
    if (usr_local_bin.exists() || usr_local_bin.is_symlink())
        && let Err(e) = std::fs::remove_file(usr_local_bin)
    {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            println!(
                "{}",
                "ℹ /usr/local/bin/fleet にシンボリックリンクを作成するには:".dimmed()
            );
            println!(
                "{}",
                format!("  sudo ln -sf {} /usr/local/bin/fleet", cargo_bin.display()).dimmed()
            );
        }
        return;
    }

    // シンボリックリンクを作成
    match symlink(&cargo_bin, usr_local_bin) {
        Ok(_) => {
            println!(
                "{}",
                format!(
                    "✓ /usr/local/bin/fleet → {} にシンボリックリンクを作成しました",
                    cargo_bin.display()
                )
                .green()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            println!(
                "{}",
                "ℹ /usr/local/bin/fleet にシンボリックリンクを作成するには:".dimmed()
            );
            println!(
                "{}",
                format!("  sudo ln -sf {} /usr/local/bin/fleet", cargo_bin.display()).dimmed()
            );
        }
        Err(_) => {
            // その他のエラーは無視（ディレクトリが存在しない等）
        }
    }
}

/// cargo install でFleetFlowを更新
async fn cargo_install_update() -> anyhow::Result<()> {
    use std::process::Command;

    println!(
        "{}",
        "🔧 cargo install --git https://github.com/chronista-club/fleetflow --force".cyan()
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

        // /usr/local/bin/fleet へのシンボリックリンクを作成
        ensure_usr_local_bin_symlink();

        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "cargo install に失敗しました（終了コード: {:?}）",
            status.code()
        ))
    }
}
