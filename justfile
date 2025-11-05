# デフォルトのレシピ一覧を表示
default:
    @just --list

# すべてのチェックを実行（CI相当）
ci: fmt-check clippy test build
    @echo "✅ すべてのチェックが完了しました"

# フォーマットチェック
fmt-check:
    @echo "📝 フォーマットをチェック中..."
    cargo fmt --all -- --check

# フォーマット適用
fmt:
    @echo "📝 フォーマットを適用中..."
    cargo fmt --all

# Clippy（リンター）
clippy:
    @echo "🔍 Clippyを実行中..."
    cargo clippy --all-targets --all-features -- -D warnings

# テスト実行
test:
    @echo "🧪 テストを実行中..."
    cargo test --all-features

# ビルド
build:
    @echo "🔨 ビルド中..."
    cargo build --all-features

# リリースビルド
build-release:
    @echo "🚀 リリースビルド中..."
    cargo build --release --all-features

# 開発前の準備（依存関係の更新など）
setup:
    @echo "🔧 開発環境をセットアップ中..."
    cargo update
    cargo fetch

# クリーンアップ
clean:
    @echo "🧹 クリーンアップ中..."
    cargo clean

# プルリクエスト前のチェック
pre-pr: fmt ci
    @echo "✨ プルリクエストの準備が完了しました"

# ドキュメント生成
doc:
    @echo "📚 ドキュメントを生成中..."
    cargo doc --all-features --no-deps --open

# セキュリティ監査（cargo-auditが必要）
audit:
    @echo "🔒 セキュリティ監査中..."
    cargo audit

# 依存関係のツリー表示
tree:
    @echo "🌳 依存関係ツリー..."
    cargo tree

# 未使用の依存関係をチェック（cargo-udepsが必要）
udeps:
    @echo "🔍 未使用の依存関係をチェック中..."
    cargo +nightly udeps --all-targets

# watchモードでテストを実行（cargo-watchが必要）
watch:
    @echo "👀 watchモードでテストを実行中..."
    cargo watch -x test

# ベンチマーク実行
bench:
    @echo "⚡ ベンチマーク実行中..."
    cargo bench
