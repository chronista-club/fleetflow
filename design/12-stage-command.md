# FleetFlow Stage Command - 設計

## アーキテクチャ

```
┌─────────────────────────────────────────────────────────┐
│                      FleetFlow CLI                       │
│                  (fleet stage up/down)                   │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  StageOrchestrator                       │
│  ┌────────────────────────────────────────────────────┐ │
│  │            Stage Type Detection                     │ │
│  │  local: Docker only / remote: Infra + Docker        │ │
│  └────────────────────────────────────────────────────┘ │
└──────────┬────────────────────────┬─────────────────────┘
           │                        │
┌──────────▼──────────┐  ┌──────────▼──────────┐
│  InfraOrchestrator  │  │  ContainerOrchestrator│
│  (remote only)      │  │  (all stages)        │
├─────────────────────┤  ├─────────────────────┤
│ - Server lifecycle  │  │ - Container up/down │
│ - Power on/off      │  │ - Logs              │
│ - DNS setup         │  │ - Status            │
└──────────┬──────────┘  └─────────────────────┘
           │
┌──────────▼──────────┐
│  CloudProvider      │
│  (sakura-cloud等)   │
└─────────────────────┘
```

## コマンド構造

### CLI定義（clap）

```rust
#[derive(Parser)]
pub enum Commands {
    /// ステージを管理
    Stage {
        #[command(subcommand)]
        command: StageCommands,
    },
    // ... 他のコマンド
}

#[derive(Subcommand)]
pub enum StageCommands {
    /// ステージを起動
    Up {
        /// ステージ名 (local, dev, pre, prod)
        stage: String,

        /// 確認プロンプトをスキップ
        #[arg(short = 'y', long)]
        yes: bool,

        /// イメージを強制的にpull
        #[arg(long)]
        pull: bool,
    },

    /// ステージを停止
    Down {
        /// ステージ名
        stage: String,

        /// サーバー電源をOFFにする（リモートステージのみ）
        #[arg(long)]
        suspend: bool,

        /// サーバーを削除する
        #[arg(long)]
        destroy: bool,

        /// 確認プロンプトをスキップ
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// ステージの状態を表示
    Status {
        /// ステージ名
        stage: String,
    },

    /// ログを表示
    Logs {
        /// ステージ名
        stage: String,

        /// 特定サービスのログのみ
        #[arg(short, long)]
        service: Option<String>,

        /// リアルタイム追従
        #[arg(short, long)]
        follow: bool,

        /// 最新N行
        #[arg(short = 'n', long, default_value = "100")]
        tail: u32,
    },

    /// コンテナ一覧
    Ps {
        /// ステージ名（省略時は全ステージ）
        stage: Option<String>,
    },
}
```

## StageOrchestrator

### 責務

1. ステージタイプの判定（local vs remote）
2. インフラとコンテナの連携オーケストレーション
3. 冪等性の保証

### 実装

```rust
pub struct StageOrchestrator {
    flow: Flow,
    container_orchestrator: ContainerOrchestrator,
    infra_orchestrator: Option<InfraOrchestrator>,
}

impl StageOrchestrator {
    pub async fn up(&self, stage_name: &str, options: UpOptions) -> Result<()> {
        let stage = self.flow.get_stage(stage_name)?;

        if stage.is_remote() {
            // 1. インフラ起動（サーバー作成/電源ON）
            self.infra_orchestrator
                .as_ref()
                .unwrap()
                .ensure_running(&stage)
                .await?;

            // 2. SSH接続待機
            self.wait_for_ssh(&stage).await?;
        }

        // 3. コンテナ起動
        self.container_orchestrator.up(&stage, &options).await?;

        // 4. DNS設定（必要なら）
        if stage.is_remote() && stage.has_dns() {
            self.infra_orchestrator
                .as_ref()
                .unwrap()
                .configure_dns(&stage)
                .await?;
        }

        Ok(())
    }

    pub async fn down(&self, stage_name: &str, options: DownOptions) -> Result<()> {
        let stage = self.flow.get_stage(stage_name)?;

        // 1. コンテナ停止
        self.container_orchestrator.down(&stage).await?;

        if stage.is_remote() {
            match (options.suspend, options.destroy) {
                (false, false) => {
                    // デフォルト: コンテナ停止のみ（サーバーは稼働継続）
                }
                (true, false) => {
                    // --suspend: サーバー電源OFF
                    self.infra_orchestrator
                        .as_ref()
                        .unwrap()
                        .power_off(&stage)
                        .await?;
                }
                (_, true) => {
                    // --destroy: サーバー削除
                    self.infra_orchestrator
                        .as_ref()
                        .unwrap()
                        .destroy(&stage)
                        .await?;
                }
            }
        }

        Ok(())
    }
}
```

## ステージタイプ判定

### 判定ロジック

```rust
impl Stage {
    /// リモートステージかどうか
    pub fn is_remote(&self) -> bool {
        !self.servers.is_empty()
    }

    /// ローカルステージかどうか
    pub fn is_local(&self) -> bool {
        self.servers.is_empty()
    }
}
```

### KDL例

```kdl
// ローカルステージ（servers なし）
stage "local" {
    service "web"
    service "db"
}

// リモートステージ（servers あり）
stage "dev" {
    server "creo-dev-01" {
        provider "sakura-cloud"
        plan core=4 memory=4
    }
    service "web"
    service "db"
}
```

## InfraOrchestrator

### 責務

1. サーバーライフサイクル管理（作成/削除）
2. 電源管理（ON/OFF）
3. DNS設定

### 実装

```rust
pub struct InfraOrchestrator {
    providers: HashMap<String, Box<dyn CloudProvider>>,
}

impl InfraOrchestrator {
    /// サーバーが起動状態であることを保証
    pub async fn ensure_running(&self, stage: &Stage) -> Result<()> {
        for server in &stage.servers {
            let provider = self.get_provider(&server.provider)?;

            match provider.get_server_status(&server.name).await? {
                ServerStatus::NotFound => {
                    // サーバーが存在しない → 作成
                    provider.create_server(server).await?;
                }
                ServerStatus::Stopped => {
                    // サーバーが停止中 → 電源ON
                    provider.power_on(&server.name).await?;
                }
                ServerStatus::Running => {
                    // 既に起動中 → 何もしない
                }
            }
        }
        Ok(())
    }

    /// サーバー電源OFF
    pub async fn power_off(&self, stage: &Stage) -> Result<()> {
        for server in &stage.servers {
            let provider = self.get_provider(&server.provider)?;
            provider.power_off(&server.name).await?;
        }
        Ok(())
    }

    /// サーバー削除
    pub async fn destroy(&self, stage: &Stage) -> Result<()> {
        for server in &stage.servers {
            let provider = self.get_provider(&server.provider)?;
            provider.destroy(&server.name).await?;
        }
        Ok(())
    }
}
```

## CloudProvider拡張

### 電源管理メソッド追加

```rust
#[async_trait]
pub trait CloudProvider {
    // 既存メソッド...

    /// サーバーの状態を取得
    async fn get_server_status(&self, name: &str) -> Result<ServerStatus>;

    /// サーバー電源ON
    async fn power_on(&self, name: &str) -> Result<()>;

    /// サーバー電源OFF（グレースフルシャットダウン）
    async fn power_off(&self, name: &str) -> Result<()>;
}

pub enum ServerStatus {
    NotFound,
    Stopped,
    Running,
    Starting,
    Stopping,
}
```

### SakuraCloudProvider

```rust
impl CloudProvider for SakuraCloudProvider {
    async fn get_server_status(&self, name: &str) -> Result<ServerStatus> {
        match self.usacloud.get_server(name).await? {
            None => Ok(ServerStatus::NotFound),
            Some(server) => {
                if server.is_running() {
                    Ok(ServerStatus::Running)
                } else {
                    Ok(ServerStatus::Stopped)
                }
            }
        }
    }

    async fn power_on(&self, name: &str) -> Result<()> {
        // 既存実装を活用
        self.usacloud.power_on(name).await
    }

    async fn power_off(&self, name: &str) -> Result<()> {
        // 既存実装を活用
        self.usacloud.power_off(name).await
    }
}
```

## 状態遷移図

```
                        ┌─────────────────┐
                        │   Not Exists    │
                        └────────┬────────┘
                                 │ stage up
                                 ▼
┌────────────┐  stage down   ┌─────────────┐
│  Stopped   │◀──(--suspend)─│   Running   │
│(power off) │               │             │
└─────┬──────┘               └──────┬──────┘
      │                             │
      │ stage up                    │ stage down
      │ (power on)                  │ (default)
      │                             ▼
      │                      ┌─────────────┐
      └─────────────────────▶│  Stopped    │
                             │(containers) │
                             └─────────────┘
                                    │
                                    │ stage down --destroy
                                    ▼
                             ┌─────────────┐
                             │   Deleted   │
                             └─────────────┘
```

## 移行戦略

### Phase 1: 並行運用

```rust
// 新旧コマンドを両方サポート
Commands::Up { stage, .. } => {
    eprintln!("⚠️  `fleet up` は非推奨です。`fleet stage up` を使用してください。");
    handle_stage_up(stage, ...).await
}

Commands::Stage { command } => {
    handle_stage_command(command).await
}
```

### Phase 2: 完全移行

1. 既存の `fleet up/down/logs/ps` を `fleet stage` へのエイリアスに変更
2. `fleet cloud` コマンドを削除
3. 非推奨警告を削除

## エラーハンドリング

### タイムアウト

```rust
const SSH_WAIT_TIMEOUT: Duration = Duration::from_secs(300); // 5分
const POWER_ON_TIMEOUT: Duration = Duration::from_secs(180); // 3分

async fn wait_for_ssh(&self, stage: &Stage) -> Result<()> {
    let start = Instant::now();
    while start.elapsed() < SSH_WAIT_TIMEOUT {
        if self.can_ssh(&stage).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    Err(Error::SshTimeout)
}
```

### ユーザー確認

```rust
async fn confirm_destroy(&self, stage: &Stage) -> Result<bool> {
    if self.options.yes {
        return Ok(true);
    }

    println!("⚠️  ステージ {} のサーバーを削除します:", stage.name);
    for server in &stage.servers {
        println!("  - {}", server.name);
    }
    println!("\nこの操作は取り消せません。");

    prompt_yes_no("続行しますか?")
}
```

## 実装優先順位

### Phase 1

1. `Commands::Stage` 列挙型追加
2. `StageCommands` サブコマンド定義
3. `handle_stage_command` 実装
4. 既存ロジックのラッパーとして動作

### Phase 2

1. `StageOrchestrator` 実装
2. `InfraOrchestrator` 実装
3. 電源管理の統合

### Phase 3

1. 既存コマンドの移行
2. 非推奨警告の追加
3. `fleet cloud` 削除
