# Claude Code 内部ツール一覧

## 概要

Claude Codeが提供する特殊な内部ツール群。これらはシステムプロンプト内でJSONSchemaとして定義されており、公開ドキュメントには詳細が記載されていない場合が多い。

---

## 1. AskUserQuestion - 対話的質問

**目的**: 実装中にユーザーと対話的に情報を収集

### 仕様

```typescript
{
  questions: Array<{
    question: string,        // 質問文（疑問符で終わる）
    header: string,          // 短いラベル（最大12文字）
    multiSelect: boolean,    // 複数選択を許可するか
    options: Array<{
      label: string,         // 選択肢の表示テキスト（1-5語）
      description: string    // 選択肢の説明
    }>  // 2-4個必須
  }>  // 1-4個
}
```

### 制約

- 質問数: 1〜4個
- オプション数: 各質問2〜4個
- 自動的に「Other」オプションが追加される

### 使用例

```typescript
AskUserQuestion({
  questions: [{
    question: "どの認証方式を使用しますか？",
    header: "Auth",
    multiSelect: false,
    options: [
      { label: "OAuth 2.0", description: "標準認証プロトコル" },
      { label: "JWT", description: "ステートレス認証" },
      { label: "API Key", description: "シンプルな認証" }
    ]
  }]
})
```

### ベストプラクティス

- ✅ 一問一答で段階的に深める
- ✅ labelは短く、descriptionで詳細を補足
- ❌ 一度に4個全部の質問を投げない

**詳細**: [ask-user-question-tool.md](./ask-user-question-tool.md)

---

## 2. Task - サブエージェント起動

**目的**: 複雑なタスクを専門エージェントに委譲

### 利用可能なエージェント

| エージェント | 目的 | 利用可能ツール |
|------------|------|--------------|
| `general-purpose` | 複雑な調査・マルチステップタスク | すべて (*) |
| `Explore` | コードベース探索 | All tools |
| `Plan` | 実装計画の立案 | All tools |
| `task-executor` | 決定論的タスク実行 | Bash, GitHub |

### thoroughness レベル（Explore/Plan）

- `quick`: 基本的な検索
- `medium`: 中程度の探索
- `very thorough`: 包括的な分析

### 仕様

```typescript
Task({
  subagent_type: string,     // 必須: エージェントタイプ
  description: string,        // 必須: 短い説明（3-5語）
  prompt: string,            // 必須: タスクの詳細指示
  model?: "sonnet" | "opus" | "haiku",  // オプション
  resume?: string            // オプション: 継続するエージェントID
})
```

### 使用例

```typescript
// コードベース探索
Task({
  subagent_type: "Explore",
  description: "Find error handlers",
  prompt: "クライアントエラーがどこで処理されているか、medium thoroughnessで探索してください"
})

// 複雑な調査タスク
Task({
  subagent_type: "general-purpose",
  description: "Research API design",
  prompt: "RESTful APIの設計パターンを調査し、このプロジェクトに適したアプローチを提案してください"
})

// CI/CD待機
Task({
  subagent_type: "task-executor",
  description: "Wait for CI completion",
  prompt: "CIが完了するまで待機し、成功したらマージしてください"
})
```

### ベストプラクティス

- ✅ **並列実行**: 独立したタスクは1つのメッセージで複数起動
- ✅ **適切なモデル**: 簡単なタスクは`haiku`でコスト削減
- ✅ **詳細な指示**: エージェントが自律的に動けるよう具体的に
- ❌ コンテキストがある場合に使用済みエージェントを再起動しない

### いつ使うべきか

**使うべき場合**:
- コードベース全体を探索する必要がある
- 複数ラウンドの検索が必要
- 長時間実行タスク（CI/CD待機など）

**使うべきでない場合**:
- 特定のファイルパスがわかっている → `Read`を使う
- 特定のクラス定義を探す → `Glob`を使う
- 特定ファイル内の検索 → `Read`を使う

---

## 3. TodoWrite - タスク管理

**目的**: タスクの計画・追跡・進捗管理

### タスク状態

- `pending`: 未着手
- `in_progress`: 作業中（同時に1つのみ）
- `completed`: 完了

### 仕様

```typescript
TodoWrite({
  todos: Array<{
    content: string,      // 必須: タスク内容（命令形）
    activeForm: string,   // 必須: 進行形（"〜中"）
    status: "pending" | "in_progress" | "completed"
  }>
})
```

### 使用例

```typescript
// タスク作成
TodoWrite({
  todos: [
    {
      content: "認証機能を実装",
      activeForm: "認証機能を実装中",
      status: "pending"
    },
    {
      content: "テストを作成",
      activeForm: "テストを作成中",
      status: "pending"
    }
  ]
})

// タスク更新（1つ目を進行中に）
TodoWrite({
  todos: [
    {
      content: "認証機能を実装",
      activeForm: "認証機能を実装中",
      status: "in_progress"
    },
    {
      content: "テストを作成",
      activeForm: "テストを作成中",
      status: "pending"
    }
  ]
})

// タスク完了
TodoWrite({
  todos: [
    {
      content: "認証機能を実装",
      activeForm: "認証機能を実装中",
      status: "completed"
    },
    {
      content: "テストを作成",
      activeForm: "テストを作成中",
      status: "in_progress"
    }
  ]
})
```

### ベストプラクティス

- ✅ **即座に完了マーク**: タスク完了後すぐに`completed`に
- ✅ **1つずつ進行**: `in_progress`は常に1つのみ
- ✅ **頻繁に使用**: 複雑なタスクは必ず使う（3ステップ以上）
- ✅ **不要なタスクは削除**: 古くなったタスクは削除
- ❌ バッチで複数完了マークしない
- ❌ トリビアルなタスク（1-2ステップ）では使わない

### いつ使うべきか

**使うべき場合**:
- 3ステップ以上の複雑なタスク
- ユーザーが複数タスクを列挙した場合
- マルチステップの計画が必要な場合

**使うべきでない場合**:
- 1-2ステップの単純なタスク
- 純粋に会話的・情報提供的なやり取り

---

## 4. ExitPlanMode - プランモード制御

**目的**: 計画フェーズから実装フェーズへの移行

### 仕様

```typescript
ExitPlanMode({
  plan: string  // 必須: 計画内容（Markdown形式）
})
```

### 使用例

```typescript
ExitPlanMode({
  plan: `
## 実装計画

1. 認証ミドルウェアの作成
   - JWT検証ロジック
   - エラーハンドリング

2. エンドポイントへの適用
   - /api/users/*
   - /api/posts/*

3. テストの追加
   - 認証成功ケース
   - 認証失敗ケース
`
})
```

### ベストプラクティス

- ✅ **曖昧さを解消**: AskUserQuestionで不明点を解決してから
- ✅ **コード実装タスクのみ**: 調査タスクでは使わない
- ❌ 複数の有効なアプローチがある場合は先に質問する

### いつ使うべきか

**使うべき場合**:
- コード実装の計画ステップを提示する必要がある
- ユーザーの承認を得てから実装に進みたい

**使うべきでない場合**:
- 調査・理解タスク（コード実装なし）
- 計画が不要な単純タスク

---

## 5. Skill - スキル実行

**目的**: 定義済みスキルの起動

### 利用可能なスキル

現在のプロジェクトでは：

- `mcp-builder`: MCP (Model Context Protocol) サーバー作成ガイド
- （他にプロジェクトで定義されたスキル）

### 仕様

```typescript
Skill({
  skill: string  // 必須: スキル名（引数なし）
})
```

### 使用例

```typescript
// MCPサーバー構築スキルを起動
Skill({
  skill: "mcp-builder"
})

// カスタムスキルを起動
Skill({
  skill: "code-flow"
})
```

### ベストプラクティス

- ✅ タスクに適したスキルがある場合は積極的に使用
- ❌ 既に実行中のスキルを再起動しない
- ❌ CLI組み込みコマンドには使わない（/help等）

---

## 6. SlashCommand - カスタムコマンド実行

**目的**: プロジェクト固有のスラッシュコマンドを実行

### 仕様

```typescript
SlashCommand({
  command: string  // 必須: /で始まるコマンド（引数含む）
})
```

### 使用例

```typescript
// カスタムコマンド実行
SlashCommand({
  command: "/review-pr 123"
})

// ドキュメント生成
SlashCommand({
  command: "/generate-docs"
})
```

### ベストプラクティス

- ✅ `.claude/commands/`で定義されたコマンドのみ使用
- ❌ 組み込みCLIコマンド（/help, /clear等）には使わない
- ❌ 利用可能リストにないコマンドは使わない
- ❌ 既に実行中のコマンドを再実行しない

### コマンドの動作

1. ユーザーまたはAIが`/command`を実行
2. `<command-message>command is running…</command-message>`が表示される
3. コマンドのプロンプトが展開される
4. 展開されたプロンプトを処理

---

## 7. WebFetch - Web コンテンツ取得

**目的**: URLからコンテンツを取得してAI処理

### 仕様

```typescript
WebFetch({
  url: string,     // 必須: 取得するURL（完全なURL）
  prompt: string   // 必須: 取得内容に対する質問
})
```

### 特徴

- HTMLをMarkdownに変換
- 小さく高速なモデルで処理
- 15分間のセルフクリーニングキャッシュ
- HTTPは自動的にHTTPSにアップグレード
- リダイレクト時は新しいURLで再リクエスト必要

### 使用例

```typescript
// ドキュメント取得
WebFetch({
  url: "https://code.claude.com/docs/en/skills",
  prompt: "スキルの作成方法を説明してください"
})

// API仕様取得
WebFetch({
  url: "https://api.example.com/docs",
  prompt: "認証エンドポイントの仕様を抽出してください"
})
```

### ベストプラクティス

- ✅ **MCP優先**: mcp__で始まるWeb fetchツールがあれば優先
- ✅ **具体的なプロンプト**: 何を抽出したいか明確に指定
- ❌ プログラミング目的以外のURL推測はしない
- ❌ ユーザー提供以外のURLを勝手に使わない

### リダイレクト処理

リダイレクトが発生した場合：

```
Redirect detected: https://example.com → https://new.example.com
```

新しいURLで再度WebFetchを実行する必要がある。

---

## 8. WebSearch - Web検索

**目的**: 最新情報の検索

### 仕様

```typescript
WebSearch({
  query: string,              // 必須: 検索クエリ
  allowed_domains?: string[], // オプション: 許可ドメイン
  blocked_domains?: string[]  // オプション: ブロックドメイン
})
```

### 制約

- 米国でのみ利用可能
- 日付を考慮（envの"Today's date"を参照）

### 使用例

```typescript
// 最新情報検索
WebSearch({
  query: "Rust async/await best practices 2025"
})

// ドメイン制限
WebSearch({
  query: "tokio tutorial",
  allowed_domains: ["tokio.rs", "docs.rs"]
})

// ドメイン除外
WebSearch({
  query: "database migration tools",
  blocked_domains: ["stackoverflow.com"]
})
```

### ベストプラクティス

- ✅ 知識カットオフ以降の情報が必要な場合に使用
- ✅ envの日付を考慮（2025年なら"2024"を避ける）
- ❌ 静的なドキュメントにはWebFetchを使う

---

## 9. BashOutput / KillShell - バックグラウンドシェル管理

**目的**: 長時間実行コマンドのモニタリングと制御

### BashOutput - 出力取得

```typescript
BashOutput({
  bash_id: string,     // 必須: シェルID
  filter?: string      // オプション: 正規表現フィルタ
})
```

### KillShell - シェル終了

```typescript
KillShell({
  shell_id: string  // 必須: シェルID
})
```

### 使用例

```typescript
// バックグラウンドでサーバー起動
Bash({
  command: "cargo run",
  run_in_background: true
})
// → bash_id: "abc123" が返される

// 出力確認
BashOutput({
  bash_id: "abc123",
  filter: "Listening on"  // "Listening on"を含む行のみ
})

// シェル終了
KillShell({
  shell_id: "abc123"
})
```

### ベストプラクティス

- ✅ 長時間実行コマンドはバックグラウンドで実行
- ✅ filterで必要な行のみ取得
- ✅ 不要になったシェルは必ず終了
- ❌ 短時間コマンドをバックグラウンド化しない

### シェルID確認

```bash
/bashes  # 実行中のシェル一覧を表示
```

---

## 10. ListMcpResourcesTool / ReadMcpResourceTool - MCP リソース管理

**目的**: MCPサーバーが提供するリソースへのアクセス

### ListMcpResourcesTool - リソース一覧

```typescript
ListMcpResourcesTool({
  server?: string  // オプション: 特定のサーバー名
})
```

### ReadMcpResourceTool - リソース読み込み

```typescript
ReadMcpResourceTool({
  server: string,  // 必須: MCPサーバー名
  uri: string      // 必須: リソースURI
})
```

### 使用例

```typescript
// 全MCPサーバーのリソース一覧
ListMcpResourcesTool({})

// 特定サーバーのリソース一覧
ListMcpResourcesTool({
  server: "notion"
})

// リソース読み込み
ReadMcpResourceTool({
  server: "notion",
  uri: "notion://page/abc123"
})
```

### ベストプラクティス

- ✅ 利用可能なリソースを先に確認
- ✅ URIは正確に指定
- ❌ 存在しないサーバー名を指定しない

---

## ツールの組み合わせパターン

### パターン1: Hearing First フロー

```typescript
// 1. ユーザーに質問
const answer = await AskUserQuestion({
  questions: [{ question: "実装タイプは？", ... }]
})

// 2. タスクリスト作成
await TodoWrite({
  todos: [
    { content: `${answer}の実装`, activeForm: "実装中", status: "pending" }
  ]
})

// 3. コードベース探索
await Task({
  subagent_type: "Explore",
  description: "Find related code",
  prompt: `${answer}に関連するコードを探索`
})
```

### パターン2: ドキュメント駆動開発

```typescript
// 1. 最新ドキュメント取得
await WebFetch({
  url: "https://docs.rs/tokio",
  prompt: "非同期タスクのベストプラクティスを要約"
})

// 2. 計画提示
await ExitPlanMode({
  plan: "取得したベストプラクティスに基づく実装計画..."
})

// 3. 実装タスク管理
await TodoWrite({
  todos: [...]
})
```

### パターン3: CI/CD 統合

```typescript
// 1. バックグラウンドでテスト実行
await Bash({
  command: "cargo test --all",
  run_in_background: true
})
// → bash_id: "test123"

// 2. 他の作業を並行実行
await Task({
  subagent_type: "general-purpose",
  description: "Update documentation",
  prompt: "ドキュメントを更新"
})

// 3. テスト結果確認
await BashOutput({
  bash_id: "test123",
  filter: "test result"
})
```

---

## まとめ

### 重要な内部ツール

| ツール | 用途 | 頻度 |
|-------|------|------|
| AskUserQuestion | 対話的情報収集 | 高 |
| TodoWrite | タスク管理 | 高 |
| Task | サブエージェント起動 | 中 |
| WebFetch | ドキュメント取得 | 中 |
| ExitPlanMode | 計画→実装移行 | 低 |
| Skill | スキル起動 | 低 |
| SlashCommand | カスタムコマンド | 低 |
| WebSearch | 最新情報検索 | 低 |
| BashOutput/KillShell | バックグラウンド管理 | 低 |

### 使い分けの原則

1. **対話が必要** → AskUserQuestion
2. **複雑なタスク** → TodoWrite + Task
3. **情報収集** → WebFetch / WebSearch
4. **計画提示** → ExitPlanMode
5. **長時間実行** → Bash(background) + BashOutput

### Hearing First における活用

```mermaid
flowchart LR
    A[ユーザー要望] --> B[AskUserQuestion]
    B --> C[TodoWrite]
    C --> D[Task/Explore]
    D --> E[ExitPlanMode]
    E --> F[実装]
```

**これらの内部ツールを効果的に組み合わせることで、ユーザーとの対話的で効率的な開発フローを実現できる。**
