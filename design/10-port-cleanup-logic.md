# Design: ポートクリーンアップの実装

## 1. ポート占有プロセスの特定

macOS/Linux 環境において、指定ポートを使用している PID を特定するために以下の手法を検討します：
- **`lsof -ti:{port}` コマンドの実行**: 依存ライブラリを増やさず、標準的なツールを利用。
- **`/proc` エントリのスキャン** (Linuxのみ): より低レイヤーなアプローチ。

初期実装では、ポータビリティとシンプルさから `lsof` または `ss`/`netstat` をコマンド経由で呼び出す方式を採用します。

## 2. シャットダウン・ループの実装

`fleetflow-container` クレートに `PortManager` または `Runtime` の拡張メソッドとして実装します。

```rust
async fn ensure_port_available(port: u16) -> Result<()> {
    let pid = find_pid_by_port(port)?;
    if let Some(pid) = pid {
        send_signal(pid, Signal::SIGTERM)?;
        
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if !is_port_in_use(port) {
                return Ok(()); // 即時キャッチ
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        
        // タイムアウト後の強制終了
        send_signal(pid, Signal::SIGKILL)?;
    }
    Ok(())
}
```

## 3. マージ順序の厳密化 (`fleetflow-core`)

`loader.rs` の `expand_all_files` における結合順序を仕様に合わせて再整理します。
現在は「ワークロード」が最初になっていますが、ステージ別設定やローカル設定の順序を再定義します。
