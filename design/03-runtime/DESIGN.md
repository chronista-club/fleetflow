# ランタイム戦略とデプロイ方針

## 基本方針

FleetFlowは**コンテナランタイム + Image**を基本とした抽象化レイヤーを提供します。

## 環境ごとのランタイム

### ローカル開発環境

**使用**: OrbStack上のDocker Engine

```bash
# ローカル開発
flow up --stage=local
```

- **ランタイム**: Docker (OrbStack)
- **ポート**: 11000番台（ホスト側）
- **データ**: ローカルファイルシステム
- **ネットワーク**: Dockerネットワーク
- **メリット**: 高速、軽量、Macネイティブ統合

### Kubernetes環境

**使用**: Kubernetes Pod

```bash
# Kubernetes環境
flow up --stage=dev    # 開発クラスタ
flow up --stage=stg    # ステージングクラスタ
```

- **ランタイム**: containerd/CRI-O
- **抽象化**: Pod = 1つ以上のコンテナ
- **ネットワーク**: Kubernetes Service
- **ストレージ**: PersistentVolumeClaim
- **スケーリング**: HorizontalPodAutoscaler

### Cloud Run環境

**使用**: Cloud Run (Google Cloud)

```bash
# Cloud Run環境
flow up --stage=prd    # 本番環境
```

- **ランタイム**: gVisor (軽量コンテナサンドボックス)
- **抽象化**: サービス単位での管理
- **ネットワーク**: HTTPSエンドポイント自動割り当て
- **スケーリング**: オートスケール（0→N）
- **料金**: リクエストベース

## 抽象化レイヤー

### Flow → Container Image

FleetFlowは環境に関わらず**コンテナイメージ**を基本単位として扱います：

```kdl
service "api" {
    image "myapp"
    version "1.0.0"
    // 環境に応じて異なるランタイムで実行される
}
```

### ランタイム別の変換

| Flow定義 | ローカル (Docker) | Kubernetes | Cloud Run |
|---------|------------------|-----------|-----------|
| service | Container | Pod | Service |
| port | HostPort:ContainerPort | Service → Pod | HTTPS Endpoint |
| volume | Bind Mount | PersistentVolume | Cloud Storage Mount |
| environment | ENV変数 | ConfigMap/Secret | Environment Variables |
| depends_on | 起動順序制御 | initContainer | N/A (HTTP依存) |

## ステージとランタイムの対応

```kdl
// local: ローカル開発 (Docker on OrbStack)
stage "local" {
    service "postgres"
    service "redis"
    service "api"
    variables {
        RUNTIME "docker"
        DEBUG "true"
    }
}

// dev: 開発クラスタ (Kubernetes)
stage "dev" {
    service "postgres"
    service "redis"
    service "api"
    variables {
        RUNTIME "kubernetes"
        NAMESPACE "dev"
        DEBUG "true"
    }
}

// stg: ステージング (Kubernetes)
stage "stg" {
    service "postgres"
    service "redis"
    service "api"
    variables {
        RUNTIME "kubernetes"
        NAMESPACE "stg"
        DEBUG "false"
    }
}

// prd: 本番 (Cloud Run)
stage "prd" {
    service "api"  // ステートレス
    variables {
        RUNTIME "cloudrun"
        PROJECT "my-project"
        REGION "asia-northeast1"
        DEBUG "false"
    }
}
```

## 実装ロードマップ

### Phase 1: Docker対応 ✅ (完了)
- [x] ローカル開発環境でのDocker管理
- [x] コンテナ作成・起動・停止
- [x] ポート管理（11000番台）
- [x] ボリューム管理

### Phase 2: Kubernetes対応 (予定)
- [ ] Kubernetes Manifestの生成
- [ ] kubectl統合
- [ ] ConfigMap/Secret管理
- [ ] Service/Ingress設定

### Phase 3: Cloud Run対応 (予定)
- [ ] Cloud Run Service定義の生成
- [ ] gcloud CLI統合
- [ ] HTTPSエンドポイント管理
- [ ] IAM権限設定

## ランタイム検出

```rust
enum ContainerRuntime {
    Docker,      // OrbStack, Docker Desktop
    Kubernetes,  // kubectl経由
    CloudRun,    // gcloud経由
}

impl ContainerRuntime {
    fn detect() -> Self {
        // 環境変数やStage設定から自動検出
        if let Ok(runtime) = std::env::var("FLEETFLOW_RUNTIME") {
            match runtime.as_str() {
                "kubernetes" => Self::Kubernetes,
                "cloudrun" => Self::CloudRun,
                _ => Self::Docker,
            }
        } else {
            Self::Docker // デフォルト
        }
    }
}
```

## ベストプラクティス

### 1. ステートレス設計

Cloud Runを考慮し、ステートを持たないサービス設計を推奨：

```kdl
// Good: ステートレス
service "api" {
    image "myapp"
    environment {
        DATABASE_URL "postgresql://cloud-sql/db"
        REDIS_URL "redis://memorystore/cache"
    }
}

// Avoid: ステートフル (ローカル/K8s専用)
service "postgres" {
    volumes {
        volume "./data" "/var/lib/postgresql/data"
    }
}
```

### 2. 環境変数での切り替え

```kdl
stage "local" {
    service "api"
    variables {
        DATABASE_URL "postgresql://localhost:11432/db"
    }
}

stage "prd" {
    service "api"
    variables {
        DATABASE_URL "postgresql://cloud-sql-proxy/db"
    }
}
```

### 3. イメージの統一

全環境で同じコンテナイメージを使用：

```bash
# ビルド
docker build -t gcr.io/project/myapp:1.0.0 .

# ローカルでテスト
flow up --stage=local

# 本番デプロイ（同じイメージ）
flow up --stage=prd
```

## 参考資料

- [OrbStack公式](https://orbstack.dev/)
- [Kubernetes公式](https://kubernetes.io/)
- [Cloud Run公式](https://cloud.google.com/run)
- [Bollard (Docker API)](https://docs.rs/bollard/)

## 更新履歴

- 2025-01-08: 初版作成
- Phase 1 (Docker対応) 完了
