---
description: vantage-mcpをcargo installでインストールする
allowed-tools: Bash(cargo:*), Bash(which:*)
---

vantage-mcpバイナリをcargoでインストールします。

## 実行内容

1. cargo installでvantage-mcpをインストール
2. インストールされたバイナリのバージョンとパスを確認
3. 結果を報告

## コマンド

```bash
# vantage-mcpをインストール
cargo install --path crates/vantage-mcp --force

# インストール確認
vantagemcp --version

# インストールパスを表示
which vantagemcp
```

インストールが成功したら、バージョン情報とインストールパスを報告してください。

エラーが発生した場合は、エラー内容を分析し、解決方法を提案してください。
