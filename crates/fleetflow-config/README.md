# fleetflow-config

[![Crates.io](https://img.shields.io/crates/v/fleetflow-config.svg)](https://crates.io/crates/fleetflow-config)
[![Documentation](https://docs.rs/fleetflow-config/badge.svg)](https://docs.rs/fleetflow-config)
[![License](https://img.shields.io/crates/l/fleetflow-config.svg)](https://github.com/chronista-club/fleetflow#license)

FleetFlowの設定ファイル検索と管理を提供するライブラリクレート。

## 概要

`fleetflow-config`は、FleetFlowの設定ファイルの検索と管理機能を提供します：

- **設定ファイル検索** - 複数の場所から自動的に設定ファイルを検索
- **設定ディレクトリ管理** - プラットフォーム固有の設定ディレクトリ
- **優先順位** - 環境変数、ローカルファイル、グローバル設定の優先順位

## 使用例

```rust
use fleetflow_config::{find_flow_file, get_config_dir};

// 設定ファイルを検索
let flow_file = find_flow_file()?;
println!("Found: {}", flow_file.display());

// 設定ディレクトリを取得
let config_dir = get_config_dir()?;
println!("Config dir: {}", config_dir.display());
```

## 設定ファイル検索の優先順位

`find_flow_file()`は以下の優先順位で設定ファイルを検索します：

1. **環境変数** `FLEETFLOW_CONFIG_PATH` - 直接パス指定
2. **カレントディレクトリ**:
   - `flow.local.kdl`
   - `.flow.local.kdl`
   - `flow.kdl`
   - `.flow.kdl`
3. **.fleetflowディレクトリ** `./.fleetflow/` 内で同様の順序
4. **グローバル設定** `~/.config/fleetflow/flow.kdl`

## 設定ディレクトリ

`get_config_dir()`は、プラットフォーム固有の設定ディレクトリを返します：

- **Linux**: `~/.config/fleetflow/`
- **macOS**: `~/Library/Application Support/fleetflow/`
- **Windows**: `%APPDATA%\fleetflow\`

ディレクトリが存在しない場合は自動的に作成されます。

## エラー処理

```rust
use fleetflow_config::{find_flow_file, ConfigError};

match find_flow_file() {
    Ok(path) => println!("Found: {}", path.display()),
    Err(ConfigError::FlowFileNotFound) => {
        eprintln!("設定ファイルが見つかりません");
        eprintln!("flow.kdl を作成してください");
    }
    Err(e) => eprintln!("エラー: {}", e),
}
```

## ドキュメント

- [FleetFlow メインプロジェクト](https://github.com/chronista-club/fleetflow)
- [API ドキュメント](https://docs.rs/fleetflow-config)

## ライセンス

MIT OR Apache-2.0
