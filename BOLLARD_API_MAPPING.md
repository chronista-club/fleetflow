# Bollard API マッピング表

## バージョン情報
- Bollard: v0.19.4
- bollard-stubs: v1.49.1-rc.28.4.0

## Container API

### Config (deprecated) → 新API

**旧API**: `bollard::container::Config<String>`
**新API**: `bollard_stubs::models::ContainerConfig`

ただし、実際のコンテナ作成には以下を使用:

```rust
// 旧
use bollard::container::{Config, CreateContainerOptions};
let config = Config { ... };
docker.create_container(Some(options), config).await?;

// 新
use bollard::container::CreateContainerOptions as CreateOpts; // まだ非推奨
use bollard_stubs::models::ContainerConfig;
// または
use bollard::models::ContainerConfig;  // bollard 0.19では両方使える

// 最新の推奨方法
use bollard::exec::CreateExecOptions;
// Docker APIに直接マッピングされたモデル
```

### CreateContainerOptions (deprecated) → 新API

**旧API**: `bollard::container::CreateContainerOptions<String>`
**新API**: なし（構造体フィールドを直接使用）

Bollard 0.19.4では、`CreateContainerOptions`はまだ使用可能だが非推奨。
新しいAPIでは、オプションパラメータは関数の引数として直接渡す。

### 実装方針

converter.rsでは以下のアプローチを取る:

1. **短期**: `#[allow(deprecated)]`で警告を抑制（現状）
2. **中期**: `bollard::models`の新しい型を使用
3. **長期**: Bollard 1.0への完全移行

## Network API

### CreateNetworkOptions (deprecated) → NetworkCreateRequest

**旧API**: `bollard::network::CreateNetworkOptions<String>`
**新API**: `bollard::models::NetworkCreateRequest` または `bollard_stubs::models::NetworkCreateRequest`

```rust
// 旧
use bollard::network::CreateNetworkOptions;
let options = CreateNetworkOptions {
    name: "my-network",
    driver: "bridge",
    ..Default::default()
};

// 新
use bollard::models::NetworkCreateRequest;
let config = NetworkCreateRequest {
    name: Some("my-network".to_string()),
    driver: Some("bridge".to_string()),
    ..Default::default()
};
```

## Image API

### CreateImageOptions (deprecated) → 新API

**旧API**: `bollard::image::CreateImageOptions<String>`
**新API**: ビルダーパターンなし。構造体を直接使用。

```rust
// 旧
use bollard::image::CreateImageOptions;
let options = CreateImageOptions {
    from_image: "alpine",
    tag: "latest",
    ..Default::default()
};

// 新 (Bollard 0.19)
// 実際には同じ構造体を使い続けるが、将来的には削除される
// パラメータとして直接渡す方式に移行予定
```

### BuildImageOptions (deprecated) → 新API

**旧API**: `bollard::image::BuildImageOptions<String>`
**新API**: 構造体を直接使用（ビルダーパターンなし）

## Logs API

### LogsOptions (deprecated) → 新API

**旧API**: `bollard::container::LogsOptions<String>`
**新API**: パラメータを直接渡す

```rust
// 旧
use bollard::container::LogsOptions;
let options = LogsOptions {
    follow: true,
    stdout: true,
    stderr: true,
    ..Default::default()
};

// 新 (将来)
// 関数パラメータとして直接渡す方式に変更される予定
```

## List Containers API

### ListContainersOptions (deprecated) → 新API

**旧API**: `bollard::container::ListContainersOptions<String>`
**新API**: パラメータを直接渡す、またはビルダーパターン（ドキュメント要確認）

## 移行戦略

### フェーズ1: converter.rs（優先度: 高）

現状の`#[allow(deprecated)]`を維持しつつ、以下を調査:
- `bollard::models::ContainerConfig`の使用可能性
- `CreateContainerOptions`の代替方法
- テストケースでの動作確認

### フェーズ2: fleetflow（優先度: 中）

各API呼び出しを個別に新APIに移行:
1. ネットワーク作成 → `NetworkCreateRequest`
2. イメージプル → パラメータ直接渡し
3. コンテナリスト → パラメータ直接渡し
4. ログ取得 → パラメータ直接渡し

### フェーズ3: fleetflow-build（優先度: 低）

`BuildImageOptions`の新API調査と移行

## 注意事項

1. **Bollard 0.19.4**: まだ多くの非推奨APIが動作する
2. **Bollard 1.0**: 非推奨APIが削除される予定
3. **後方互換性**: 新APIへの移行は破壊的変更になる可能性がある

## 参考

- [Bollard Documentation](https://docs.rs/bollard/0.19.4/bollard/)
- [Bollard GitHub](https://github.com/fussybeaver/bollard)
- [Docker Engine API](https://docs.docker.com/engine/api/)
