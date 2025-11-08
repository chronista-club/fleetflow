# Hello World Example

FleetFlowを使った2種類のHello Worldサンプルです。

## テストシナリオ

このディレクトリで以下のコマンドを実行してテストします。

### 1. ワンショット実行（終了するコンテナ）

メッセージを表示して終了するコンテナをテストします。

```bash
cd examples/hello-world

# hello-oneshotサービスを実行
flow up oneshot

# 期待される動作:
# 1. alpineコンテナが起動
# 2. "Hello World from FleetFlow!" を表示
# 3. コンテナが終了
```

### 2. 継続実行（起動し続けるコンテナ）

Webサーバーとして起動し続けるコンテナをテストします。

```bash
cd examples/hello-world

# hello-nginxサービスを起動
flow up nginx

# ブラウザでアクセス
open http://localhost:8080

# サービスを停止
flow down nginx
```

### 3. 両方同時に実行

```bash
cd examples/hello-world

# 両方のサービスを起動（デフォルトステージ）
flow up

# 期待される動作:
# - hello-oneshot: メッセージ表示後に終了
# - hello-nginx: ポート8080で起動し続ける
```

## 設定

このサンプルは `flow.kdl` で以下のように定義されています：

```kdl
// 一度実行して終了するコンテナ
service "hello-oneshot" {
    image "alpine"
    version "latest"
    command "echo 'Hello World from FleetFlow!'"
}

// 継続して起動するコンテナ
service "hello-nginx" {
    image "nginx"
    version "alpine"
    ports {
        port 8080 80
    }
    volumes {
        volume "." "/usr/share/nginx/html" read_only=true
    }
}

// ワンショット実行のみ
stage "oneshot" {
    service "hello-oneshot"
}

// Nginx起動のみ
stage "nginx" {
    service "hello-nginx"
}

// 両方同時実行（デフォルト）
stage "default" {
    service "hello-oneshot"
    service "hello-nginx"
}
```

## ポイント

### hello-oneshot（終了するコンテナ）
- **軽量**: `alpine:latest` イメージ（約5MB）
- **シンプル**: echoコマンドで終了
- **用途**: バッチ処理、初期化スクリプト、ワンタイムタスクのテスト

### hello-nginx（起動し続けるコンテナ）
- **軽量**: `nginx:alpine` イメージ
- **ポートマッピング**: ホストの8080→コンテナの80
- **ボリュームマウント**: ローカルHTMLを読み取り専用でマウント
- **用途**: Webサーバー、API、常駐サービスのテスト

## カスタマイズ

`index.html` を編集して、コンテナを再起動すれば変更が反映されます：

```bash
# 編集
vim index.html

# 再起動
flow down nginx
flow up nginx
```

## ファイル構成

```
examples/hello-world/
├── flow.kdl          # サービス定義
├── index.html        # Hello Worldページ
└── README.md         # このファイル
```
