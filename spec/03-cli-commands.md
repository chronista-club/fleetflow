# CLIコマンド - 仕様書

## コンセプト

### ビジョン

Docker Composeよりもシンプルで直感的なコマンド体系を提供し、開発者が迷わず使えるCLIツールを目指す。

### 哲学・設計原則

#### 1. **最小限のコマンド数**

Docker Composeの冗長さを排除し、本当に必要なコマンドだけを提供する。

- `up`: 起動
- `down`: 停止
- `logs`: ログ確認
- `ps`: 状態確認

これだけで日常の90%の作業をカバーできる。

#### 2. **直感的なオプション設計**

ユーザーが「こういうオプションがあるはず」と思ったものが実際に存在する設計。

```bash
# 削除したい → --remove
flow down --remove

# ログを追いたい → --follow
flow logs --follow

# 全部見たい → --all
flow ps --all
```

#### 3. **環境変数との統合**

`FLEETFLOW_STAGE`環境変数でステージを指定できることで、繰り返し入力の手間を省く。

```bash
# 毎回 --stage=local を打つ必要がない
export FLEETFLOW_STAGE=local
flow up
flow logs
flow down
```

#### 4. **エラーメッセージの親切さ**

エラーが起きた時に「次に何をすればいいか」が分かるメッセージを表示する。

```
✗ ポートが既に使用されています

原因:
  port 5432 is already allocated

解決方法:
  • 既存のコンテナを停止: flow down --stage=local
  • 別のポート番号を使用してください
```

### 他との違い

#### vs Docker Compose

| 観点 | Docker Compose | Flow |
|------|---------------|------|
| コマンド数 | 20+ | 4つのコア |
| 設定形式 | YAML | KDL |
| エラー表示 | 技術的 | 解決策付き |
| ステージ管理 | profiles | 第一級概念 |

#### vs Kubernetes

| 観点 | Kubernetes | Flow |
|------|-----------|------|
| 複雑さ | 高い | 低い |
| ローカル対応 | 弱い | 強い |
| 学習曲線 | 急 | 緩やか |

## 仕様

### 機能仕様

#### FS-001: flow up - コンテナ起動

**目的**: 指定したステージのサービスを起動する

**入力**:
- `--stage <STAGE>`: ステージ名（必須）
- 環境変数 `FLEETFLOW_STAGE`: ステージ名（オプション）

**出力**:
- 起動プロセスの進行状況
- 各サービスの起動結果
- エラー時は詳細なメッセージと解決策

**振る舞い**:
1. flow.kdl を読み込む
2. 指定されたステージの定義を取得
3. Docker に接続
4. 各サービスに対して：
   - コンテナが存在しない → 作成して起動
   - コンテナが存在する → 既存を起動
   - コンテナが既に起動中 → スキップ

**制約**:
- ステージ名は必須
- Docker が起動している必要がある
- イメージが存在する必要がある

#### FS-002: flow down - コンテナ停止

**目的**: 指定したステージのサービスを停止する

**入力**:
- `--stage <STAGE>`: ステージ名（必須）
- `--remove`: コンテナを削除するフラグ（オプション）
- 環境変数 `FLEETFLOW_STAGE`: ステージ名（オプション）

**出力**:
- 停止プロセスの進行状況
- 各サービスの停止結果
- `--remove` が指定された場合は削除結果

**振る舞い**:
1. flow.kdl を読み込む
2. 指定されたステージの定義を取得
3. Docker に接続
4. 各サービスに対して：
   - コンテナを停止
   - `--remove` が指定されていれば削除

**制約**:
- ステージ名は必須
- Docker が起動している必要がある

#### FS-003: flow logs - ログ表示

**目的**: コンテナのログを表示する

**入力**:
- `--stage <STAGE>`: ステージ名（サービス名と排他）
- `--service <SERVICE>`: サービス名（ステージ名と排他）
- `--lines <N>`: 表示行数（デフォルト: 100）
- `--follow`: リアルタイム追跡モード
- 環境変数 `FLEETFLOW_STAGE`: ステージ名（オプション）

**出力**:
- サービスごとに色分けされたログ
- タイムスタンプ付き
- stdout と stderr の区別

**振る舞い**:
1. 対象サービスを決定（ステージまたはサービス指定）
2. Docker に接続
3. 各サービスのログを取得
4. 色分けして表示：
   - サービス名プレフィックス: 色分け
   - stderr: 赤色の "stderr:" ラベル
   - stdout: 通常表示

**制約**:
- ステージ名またはサービス名のいずれかが必須
- Docker が起動している必要がある
- コンテナが存在する必要がある

#### FS-004: flow ps - コンテナ一覧表示

**目的**: 管理中のコンテナの状態を確認する

**入力**:
- `--stage <STAGE>`: ステージでフィルタ（オプション）
- `--all`: 停止中のコンテナも表示
- 環境変数 `FLEETFLOW_STAGE`: ステージ名（オプション）

**出力**:
- 表形式のコンテナ一覧
  - NAME: コンテナ名
  - STATUS: 実行状態（色付き）
  - IMAGE: イメージ名
  - PORTS: ポート情報

**振る舞い**:
1. Docker に接続
2. flow- プレフィックスのコンテナを取得
3. ステージが指定されていればフィルタ
4. 表形式で表示

**制約**:
- Docker が起動している必要がある

#### FS-005: flow deploy - ステージデプロイ（CI/CD向け）

**目的**: 既存コンテナを強制停止・削除し、最新イメージで再起動する

**入力**:
- `<STAGE>`: ステージ名（必須）
- `-n, --service <SERVICE>`: デプロイ対象のサービス（省略時は全サービス）
- `--no-pull`: イメージのpullをスキップ（デフォルトは常にpull）
- `-y, --yes`: 確認なしで実行
- 環境変数 `FLEETFLOW_STAGE`: ステージ名（オプション）

**出力**:
- デプロイプロセスの進行状況
- 各ステップ（停止・削除・起動）の結果
- エラー時は詳細なメッセージ

**振る舞い**:
1. flow.kdl を読み込む
2. 指定されたステージの定義を取得
3. `--service` が指定されていれば対象をフィルタ
4. `--yes` がなければ確認メッセージを表示して終了
5. Docker に接続
6. 【Step 1/3】既存コンテナを停止・削除
   - 各サービスのコンテナを停止
   - 各サービスのコンテナを強制削除（ポート解放）
7. 【Step 2/3】最新イメージをダウンロード（`--no-pull` でスキップ可能）
8. 【Step 3/3】依存関係順にコンテナを作成・起動

**制約**:
- ステージ名は必須
- Docker が起動している必要がある
- `--yes` オプションがないと実行されない（安全装置）

**使用例**:
```bash
# 全サービスをデプロイ（最新イメージを自動pull）
flow deploy prod --yes

# 特定サービスのみデプロイ
flow deploy prod --service db --yes

# ローカルイメージを使用（pullスキップ）
flow deploy prod --yes --no-pull
```

#### FS-006: flow build - イメージビルド

**目的**: Dockerイメージをビルドし、オプションでレジストリにプッシュする

**入力**:
- `<STAGE>`: ステージ名（必須）
- `-n, --service <SERVICE>`: ビルド対象のサービス（省略時は全サービス）
- `--push`: ビルド後にレジストリにプッシュ
- `--tag <TAG>`: イメージタグを指定
- `--registry <REGISTRY>`: レジストリURL（例: `ghcr.io/owner`）
- `--platform <PLATFORM>`: ターゲットプラットフォーム（例: `linux/amd64`）
- `--no-cache`: キャッシュを使用しない

**出力**:
- ビルドプロセスの進行状況
- 各サービスのビルド結果
- プッシュ時はプッシュ先URL

**振る舞い**:
1. flow.kdl を読み込む
2. 指定されたステージの定義を取得
3. ビルド対象のサービスを決定（build 設定があるもののみ）
4. **localステージの場合**:
   - ネイティブプラットフォームでビルド（bollard API使用）
5. **local以外のステージの場合**:
   - docker buildx を使用してクロスプラットフォームビルド
   - デフォルトプラットフォーム: `linux/amd64`
   - `--push` 指定時はビルドと同時にプッシュ
6. イメージ名形式:
   - `--registry` 指定時: `{registry}/{project}-{stage}:{tag}`
   - `--registry` 未指定時: 従来のイメージ名

**制約**:
- ステージ名は必須
- Docker が起動している必要がある
- `--registry` + `--push` でクラウドへのプッシュが可能
- local以外でpush時は docker buildx が必要

**使用例**:
```bash
# ローカル開発用ビルド（ネイティブ）
flow build local

# dev環境用: ghcr.ioにpush（linux/amd64）
flow build dev --registry ghcr.io/myorg --push

# prod環境用: 特定サービスのみビルド＆push
flow build prod --registry ghcr.io/myorg --push --service api

# キャッシュなしでリビルド
flow build prod --registry ghcr.io/myorg --push --no-cache
```

### インターフェース仕様

```bash
# 基本的な使用方法
flow up --stage=local
flow down --stage=local
flow logs --stage=local
flow ps --stage=local

# 環境変数を使用
export FLEETFLOW_STAGE=local
flow up
flow logs --follow
flow down --remove

# サービス指定
flow logs --service=postgres --lines=1000
```

### 非機能仕様

#### パフォーマンス

- コマンド起動時間: < 100ms
- Docker API 呼び出し: 並行処理で最適化
- ログストリーミング: バッファリングで遅延最小化

#### セキュリティ

- Docker socket への接続は読み取り専用を推奨
- 環境変数の取り扱いに注意
- エラーメッセージに機密情報を含めない

####互換性

- Rust 2024 edition
- Docker API: 1.40+
- OrbStack / Docker Desktop 対応

## 哲学的考察

### なぜこの仕様か

#### シンプルさの追求

Docker Compose は機能が多すぎて、ほとんどの人は一部しか使わない。
Flowは「本当に必要な機能」だけを提供することで、学習コストを下げ、迷いを減らす。

#### エラーは学びの機会

エラーメッセージは単なる「失敗通知」ではなく、ユーザーを次のステップに導くガイドである。
「何が起きたか」だけでなく「どうすればいいか」を伝える。

#### 色は情報

色は装飾ではなく、情報の分類手段である。
- 緑: 成功
- 赤: エラー
- 黄: 警告
- 青: 情報
- シアン/マゼンタ: サービスの識別

複数サービスのログを見る時、色がなければ混乱する。

### ユーザー体験

#### 初めて使う人

```bash
$ flow up
✗ ステージ名を指定してください: --stage=local または FLEETFLOW_STAGE=local
```

この一文で「何をすればいいか」が分かる。

#### 日常的に使う人

```bash
$ export FLEETFLOW_STAGE=local
$ flow up
$ flow logs -f
# 開発作業
$ flow down
```

わずか4つのコマンドで開発サイクルが回る。

#### トラブルシューティング

```bash
$ flow up
✗ ポートが既に使用されています

解決方法:
  • 既存のコンテナを停止: flow down --stage=local
```

次に何をすればいいかが明確。

### 進化の方向性

#### Phase 1: ローカル開発（完了）
- Docker 対応
- 基本的なコマンド

#### Phase 2: リモート環境（予定）
- Kubernetes 対応
- Cloud Run 対応

#### Phase 3: チーム開発（予定）
- 共有ステージ
- ログ集約
- メトリクス

#### 将来的な拡張

```bash
# ステージ間のデータ移行
flow migrate --from=local --to=dev

# ヘルスチェック
flow health --stage=local

# リソース使用状況
flow stats --stage=local
```

ただし、これらは「本当に必要」と確信できた時だけ追加する。
シンプルさは機能を追加することよりも、追加しないことで守られる。
