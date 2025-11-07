# ポート番号の規約

unison-flowプロジェクトでは、ホスト側のポート番号として **11000番台** を使用します。

## 標準ポートマッピング

| サービス | コンテナポート | ホストポート | 用途 |
|---------|--------------|------------|------|
| PostgreSQL | 5432 | **11432** | データベース |
| Redis | 6379 | **11379** | キャッシュ |
| App (HTTP) | 8080 | **11080** | アプリケーション |
| SurrealDB | 8000 | **11800** | データベース |
| Qdrant (REST) | 6333 | **11633** | ベクトルDB |
| Qdrant (gRPC) | 6334 | **11634** | ベクトルDB |

## 理由

- **ポート競合の回避**: 他のプロセスやサービスとのポート競合を避ける
- **識別性**: unison-flowで起動したサービスであることを明確にする
- **管理性**: 11000番台という規則で統一することで管理しやすい
- **予測可能性**: 開発者が容易にポート番号を推測できる

## KDL記述例

```kdl
service "postgres" {
    version "16"
    ports {
        port host=11432 container=5432
    }
}

service "redis" {
    version "7"
    ports {
        port host=11379 container=6379
    }
}

service "api" {
    image "myapp"
    ports {
        port host=11080 container=8080
    }
}
```

## ポート番号の計算ルール

基本的に `11000 + (元のポート番号の下3桁)` という形式で統一しています：

- 5432 → 11432 (11000 + 432)
- 6379 → 11379 (11000 + 379)
- 8080 → 11080 (11000 + 080)
- 8000 → 11800 (11000 + 800)
- 6333 → 11633 (11000 + 633)
- 6334 → 11634 (11000 + 634)

## 接続例

```bash
# PostgreSQL
psql -h localhost -p 11432 -U flowuser flowdb

# Redis
redis-cli -h localhost -p 11379

# HTTP API
curl http://localhost:11080

# SurrealDB
surreal connect http://localhost:11800

# Qdrant REST API
curl http://localhost:11633
```
