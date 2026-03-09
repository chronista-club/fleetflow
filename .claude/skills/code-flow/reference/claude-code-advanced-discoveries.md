# Claude Code 高度な発見 - ハッカー調査レポート

## 🎯 調査概要

並列で起動した複数の凄腕ハッカーエージェントによる、Claude Code内部ツールと隠し機能の包括的な調査結果。

**調査日**: 2025-11-18
**調査方法**:
- general-purposeエージェント: システムプロンプト深層分析
- Exploreエージェント: プロジェクト内ツール参照探索（very thorough）

---

## 📦 完全ツールマッピング

### 1. コア開発ツール（標準）

| ツール | 目的 | 特殊機能 |
|--------|------|---------|
| **Bash** | シェル実行 | バックグラウンド実行（`run_in_background: true`） |
| **Read** | ファイル読み込み | 画像・PDF・Jupyter notebook対応 |
| **Write** | ファイル書き込み | 新規作成（既存ファイルは上書き） |
| **Edit** | 文字列置換編集 | Unicode対応、タブ/スペース厳密保持 |
| **Glob** | ファイルパターン検索 | 変更時刻順ソート |
| **Grep** | コンテンツ検索 | ripgrepベース、multiline対応 |
| **NotebookEdit** | Jupyter編集 | セル単位の編集（replace/insert/delete） |

### 2. MCPサーバー統合ツール（重要な発見）

#### Chrome DevTools MCP (`mcp__chrome-devtools__*`) 🔥

**完全なブラウザ自動化スイート**:

```typescript
// ページ操作
take_snapshot()           // a11yツリーベースのテキストスナップショット
take_screenshot()         // ページ/エレメント単位のスクリーンショット
click({uid, dblClick?})
fill({uid, value})
fill_form([{uid, value}]) // 複数要素の一括入力
hover({uid})
drag({from_uid, to_uid})
press_key({key})          // キーボード操作
upload_file({uid, filePath})

// ページ管理
navigate_page({type: "url"|"back"|"forward"|"reload", url?})
new_page({url})
select_page({pageIdx})
close_page({pageIdx})
list_pages()
resize_page({width, height})

// ネットワーク監視
list_network_requests({
  resourceTypes?: ["xhr", "fetch", "document", ...],
  pageSize?: number,
  includePreservedRequests?: boolean
})
get_network_request({reqid?})

// コンソール監視
list_console_messages({
  types?: ["log", "error", "warn", ...],
  pageSize?: number
})
get_console_message({msgid})

// パフォーマンス計測
performance_start_trace({reload: boolean, autoStop: boolean})
performance_stop_trace()
performance_analyze_insight({insightSetId, insightName})

// エミュレーション
emulate({
  cpuThrottlingRate?: 1-20,
  networkConditions?: "Slow 3G" | "Fast 3G" | ...
})

// その他
evaluate_script({function, args?})  // JavaScript実行
wait_for({text, timeout?})
handle_dialog({action: "accept"|"dismiss", promptText?})
```

**活用アイデア**:
- WebアプリのE2Eテスト自動生成
- パフォーマンスボトルネック診断
- Core Web Vitals自動レポート
- UI操作の自動化とスクリーンショット取得

#### Notion MCP (`mcp__notion__*`) 📝

**エンタープライズドキュメント管理**:

```typescript
// 検索
notion-search({
  query: string,
  query_type: "internal" | "user",
  page_url?: string,
  data_source_url?: string,
  teamspace_id?: string,
  filters?: {
    created_date_range?: {start_date, end_date},
    created_by_user_ids?: string[]
  }
})

// ページ/データベース取得
notion-fetch({id: string})  // URL or ID

// ページ作成（Notion-flavored Markdown）
notion-create-pages({
  parent?: {page_id | database_id | data_source_id},
  pages: [{
    properties: {...},
    content: string  // Notion-flavored Markdown
  }]
})

// ページ更新
notion-update-page({
  page_id: string,
  data: {
    command: "update_properties" | "replace_content" |
             "replace_content_range" | "insert_content_after",
    properties?: {...},
    new_str?: string,
    selection_with_ellipsis?: string
  }
})

// データベース操作
notion-create-database({parent?, title?, properties: {...}})
notion-update-database({database_id, title?, description?, properties?, in_trash?})

// コメント
notion-create-comment({parent: {page_id}, rich_text: [...]})
notion-get-comments({page_id})

// ページ操作
notion-move-pages({page_or_database_ids: [...], new_parent: {...}})
notion-duplicate-page({page_id})

// 管理
notion-get-teams({query?})
notion-get-users({query?, page_size?, start_cursor?})
notion-list-agents({query?})  // カスタムワークフロー
notion-get-self()
```

**隠れた機能**:
- **拡張プロパティタイプ**:
  ```json
  {
    "date:Due Date:start": "2024-12-25",
    "date:Due Date:end": "2024-12-26",
    "date:Due Date:is_datetime": 0,
    "place:Office:name": "HQ",
    "place:Office:latitude": 37.7749,
    "place:Office:longitude": -122.4194
  }
  ```

- **特殊ブロック**:
  - `<synced_block>`: コンテンツ同期
  - `<meeting-notes>`: AI要約 + 文字起こし統合
  - `<table_of_contents>`: 動的目次
  - `<columns>`: マルチカラムレイアウト

#### Atlassian MCP (`mcp__atlassian__*`) 🏢

**Confluence統合**:

```typescript
// スペース/ページ取得
getConfluenceSpaces({cloudId, ids?, keys?, type?, status?, ...})
getConfluencePage({cloudId, pageId})
getPagesInConfluenceSpace({cloudId, spaceId, status?, subtype?: "live"|"page", ...})
getConfluencePageDescendants({cloudId, pageId, depth?})

// ページ作成・更新（Markdown形式）
createConfluencePage({cloudId, spaceId, parentId?, title?, body, subtype?: "live", isPrivate?})
updateConfluencePage({cloudId, pageId, title?, body, versionMessage?, status?: "current"|"draft"})

// コメント
getConfluencePageFooterComments({cloudId, pageId, status?, sort?})
getConfluencePageInlineComments({cloudId, pageId, resolutionStatus?: "open"|"resolved", ...})
createConfluenceFooterComment({cloudId, pageId?, body, parentCommentId?})
createConfluenceInlineComment({cloudId, pageId?, body, inlineCommentProperties?, parentCommentId?})

// 検索（CQL: Confluence Query Language）
searchConfluenceUsingCql({cloudId, cql, limit?, cursor?})
// 例: "title ~ 'meeting' AND type = page"
```

**Jira統合**:

```typescript
// イシュー操作
getJiraIssue({cloudId, issueIdOrKey, fields?, expand?})
createJiraIssue({cloudId, projectKey, issueTypeName, summary, description?, assignee_account_id?, parent?})
editJiraIssue({cloudId, issueIdOrKey, fields: {...}})

// ワークフロー
getTransitionsForJiraIssue({cloudId, issueIdOrKey})
transitionJiraIssue({cloudId, issueIdOrKey, transition: {id}, fields?, update?})

// 検索（JQL: Jira Query Language）
searchJiraIssuesUsingJql({cloudId, jql, fields?, maxResults?, nextPageToken?})
// 例: "project = PROJ AND status = 'In Progress'"

// コメント・リンク
addCommentToJiraIssue({cloudId, issueIdOrKey, commentBody, commentVisibility?})
getJiraIssueRemoteIssueLinks({cloudId, issueIdOrKey, globalId?})

// プロジェクト管理
getVisibleJiraProjects({cloudId, action?: "view"|"browse"|"edit"|"create", searchString?})
getJiraProjectIssueTypesMetadata({cloudId, projectIdOrKey})
getJiraIssueTypeMetaWithFields({cloudId, projectIdOrKey, issueTypeId})

// ユーザー検索
lookupJiraAccountId({cloudId, searchString})

// Rovo Search（横断検索）
search({query})  // JiraとConfluenceを横断検索
fetch({id})      // ARIで取得（ari:cloud:jira:... or ari:cloud:confluence:...）
```

**重要な制約**: `cloudId`はUUID、サイトURL、またはAtlassian URLから抽出可能

#### Akasha Memory MCP (`mcp__akasha-memory__*`) 🧠

```typescript
addMemory({thingToRemember: string})
search({informationToGet: string})
whoAmI()
```

**注意**: プロジェクトスコープは`x-akasha-project`ヘッダーで制御

#### Serena MCP (`mcp__serena__*`) 🔍

**コード解析特化 + メタ認知機能**

```typescript
// ファイル操作
read_file({relative_path, start_line?, end_line?, max_answer_chars?})
create_text_file({relative_path, content})
list_dir({relative_path, recursive, max_answer_chars?})
find_file({file_mask, relative_path})

// 高度な検索
search_for_pattern({
  substring_pattern: string,  // 正規表現
  relative_path?: string,
  restrict_search_to_code_files?: boolean,
  paths_include_glob?: string,
  paths_exclude_glob?: string,
  context_lines_before?: number,
  context_lines_after?: number
})

// シンボル操作
get_symbols_overview({relative_path})
find_symbol({
  name_path: string,  // "/class/method" or "class/method" or "method"
  relative_path?: string,
  depth?: number,
  include_body?: boolean,
  substring_matching?: boolean,
  include_kinds?: number[],
  exclude_kinds?: number[]
})
find_referencing_symbols({name_path, relative_path, include_kinds?, exclude_kinds?})

// シンボル編集
replace_symbol_body({name_path, relative_path, body})
insert_after_symbol({name_path, relative_path, body})
insert_before_symbol({name_path, relative_path, body})
replace_regex({
  relative_path,
  regex: string,
  repl: string,
  allow_multiple_occurrences?: boolean
})

// メモリ機能
write_memory({memory_name, content, max_answer_chars?})
read_memory({memory_file_name, max_answer_chars?})
list_memories()
delete_memory({memory_file_name})

// シェル実行
execute_shell_command({command, cwd?, capture_stderr?, max_answer_chars?})

// プロジェクト管理
activate_project({project: string})
switch_modes({modes: string[]})  // 例: ["editing", "interactive"]
check_onboarding_performed()
onboarding()

// メタ認知ツール（🔥重要な発見）
think_about_collected_information()
think_about_task_adherence()
think_about_whether_you_are_done()

// その他
prepare_for_new_conversation()
```

**メタ認知機能の重要性**:
- `think_about_collected_information`: 収集した情報が十分か評価
- `think_about_task_adherence`: タスクから逸脱していないか確認
- `think_about_whether_you_are_done`: 完了判定の自己評価

→ **AIの自己評価メカニズムとして機能**

### 3. タスク管理・対話ツール

#### TodoWrite - FSM（有限状態機械）として機能

**重要な発見**:
```
制約:
- Exactly ONE task must be in_progress at any time
- ONLY mark as completed when FULLY accomplished
- Never mark completed if:
  - Tests are failing
  - Implementation is partial
  - Unresolved errors exist
```

→ タスクの完全性を保証する状態機械

#### AskUserQuestion - 対話的質問

詳細は [ask-user-question-tool.md](./ask-user-question-tool.md) を参照

### 4. Web関連

#### WebFetch - キャッシング機構付き

```typescript
WebFetch({
  url: string,
  prompt: string
})
```

**特殊機能**:
- 15分間のセルフクリーニングキャッシュ
- HTMLをMarkdownに自動変換
- リダイレクト検出（再リクエスト必要）

#### WebSearch - 地域制限あり

```typescript
WebSearch({
  query: string,
  allowed_domains?: string[],
  blocked_domains?: string[]
})
```

**制約**: 米国でのみ利用可能

### 5. その他の内部ツール

- **Skill**: スキル実行
- **SlashCommand**: カスタムコマンド実行
- **ListMcpResourcesTool / ReadMcpResourceTool**: MCPリソース管理
- **BashOutput / KillShell**: バックグラウンドシェル管理
- **ExitPlanMode**: プランモード→実装モード遷移

---

## 🎨 高度な使用法とパターン

### パターン1: Git操作の自動化

**Git Safety Protocol**（システムプロンプトで定義）:
```
- NEVER update git config
- NEVER run destructive commands (push --force, hard reset)
- NEVER skip hooks (--no-verify, --no-gpg-sign)
- NEVER force push to main/master
- Avoid git commit --amend (例外: pre-commit hook修正時)
```

**コミットフロー**:
```bash
# 並列実行
git status & git diff & git log

# コミットメッセージテンプレート
git commit -m "$(cat <<'EOF'
変更内容の要約

## コード変更
- ファイル1: 変更内容

## ドキュメント更新
- docs/spec/XX/SPEC.md: 更新内容

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>
EOF
)"
```

**Pre-commit hook対応**:
1. コミット失敗時は1回リトライ
2. authorship確認: `git log -1 --format='%an %ae'`
3. push前確認: `git status`で"Your branch is ahead"
4. 両方trueならamend、そうでなければ新規コミット

### パターン2: PR作成の自動化

```typescript
// 1. ブランチ全体のdiff確認
Bash("git log main...HEAD")
Bash("git diff main...HEAD")

// 2. PRテンプレート
gh pr create --title "..." --body "$(cat <<'EOF'
## Summary
<1-3 bullet points>

## Test plan
[Bulleted markdown checklist]

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

### パターン3: Chrome DevTools - パフォーマンス診断

```typescript
// 1. エミュレーション設定
emulate({
  cpuThrottlingRate: 4,
  networkConditions: "Slow 3G"
})

// 2. トレース開始
performance_start_trace({
  reload: true,
  autoStop: false
})

// 3. ユーザー操作シミュレーション
click({uid: "login-button"})
fill({uid: "email", value: "test@example.com"})
click({uid: "submit"})

// 4. トレース停止
performance_stop_trace()

// 5. インサイト分析
performance_analyze_insight({
  insightSetId: "...",
  insightName: "LCPBreakdown"
})
// → Core Web Vitals取得
```

### パターン4: E2Eテスト自動生成

```typescript
// 1. 初期状態スナップショット
take_snapshot()

// 2. フォーム一括入力
fill_form([
  {uid: "name", value: "John Doe"},
  {uid: "email", value: "john@example.com"},
  {uid: "password", value: "secure123"}
])

// 3. 送信
click({uid: "submit-button"})

// 4. 成功待機
wait_for({text: "Success", timeout: 5000})

// 5. ネットワークリクエスト確認
list_network_requests({
  resourceTypes: ["xhr", "fetch"]
})

// 6. スクリーンショット取得
take_screenshot({fullPage: true})
```

### パターン5: 自動ドキュメント生成パイプライン

```typescript
// 1. Serenaでコードベース解析
const symbols = await find_symbol({
  name_path: "/",
  depth: 2,
  include_body: false
})

// 2. Notionページ作成
await notion-create-pages({
  parent: {page_id: "..."},
  pages: symbols.map(sym => ({
    properties: {title: sym.name},
    content: `# ${sym.name}\n\n${sym.documentation}`
  }))
})

// 3. Confluenceにも同期
for (const sym of symbols) {
  await createConfluencePage({
    cloudId: "...",
    spaceId: "...",
    title: sym.name,
    body: `# ${sym.name}\n\n${sym.documentation}`
  })
}
```

### パターン6: Jira-GitHub統合ワークフロー

```typescript
// 1. Jiraイシュー検索
const issues = await searchJiraIssuesUsingJql({
  cloudId: "...",
  jql: "project = PROJ AND status = 'To Do' AND assignee = currentUser()"
})

// 2. ブランチ作成
for (const issue of issues) {
  await Bash(`git checkout -b feature/${issue.key}`)

  // 3. コード実装（Task or 直接編集）

  // 4. コミット
  await Bash(`git add . && git commit -m "${issue.key}: ${issue.fields.summary}"`)

  // 5. PR作成
  await Bash(`gh pr create --title "${issue.key}: ${issue.fields.summary}"`)

  // 6. Jiraステータス更新
  await transitionJiraIssue({
    cloudId: "...",
    issueIdOrKey: issue.key,
    transition: {id: "21"}  // "In Review"
  })
}
```

### パターン7: クロスプラットフォームメモリ同期

```typescript
// 1. Akasha Memoryに追加
await addMemory({
  thingToRemember: "新しいAPI設計パターン: ..."
})

// 2. Serena memoryに保存
await write_memory({
  memory_name: "api-design-patterns",
  content: "# API設計パターン\n\n..."
})

// 3. Notionページ作成
await notion-create-pages({
  pages: [{
    properties: {title: "API設計パターン"},
    content: "# API設計パターン\n\n..."
  }]
})
```

---

## 🔬 実験的機能・ベータ機能

### 1. Bashバックグラウンド実行

```typescript
// 長時間ビルド
const {bash_id} = await Bash({
  command: "cargo build --release",
  run_in_background: true,
  timeout: 600000  // 最大10分
})

// 他の作業を並行実行
await Task({...})

// 出力確認（正規表現フィルタ付き）
await BashOutput({
  bash_id,
  filter: "Compiling|Finished"
})

// 終了
await KillShell({shell_id: bash_id})
```

### 2. Grepマルチライン検索

```typescript
Grep({
  pattern: "struct.*?\\{[\\s\\S]*?field",
  multiline: true,  // 複数行マッチング有効化
  output_mode: "content",
  "-C": 2  // 前後2行のコンテキスト
})
```

### 3. Chrome DevToolsドラッグ&ドロップ

```typescript
drag({
  from_uid: "draggable-item",
  to_uid: "drop-zone"
})
```

### 4. Notion Agentワークフロー（将来機能）

```typescript
// カスタムワークフローエージェント一覧
const agents = await notion-list-agents({query: "..."})
// → 将来的にエージェント実行機能が追加される可能性
```

### 5. Serena モード切り替え

```typescript
switch_modes({
  modes: ["editing", "interactive"]
})
// または
switch_modes({
  modes: ["planning", "one-shot"]
})
```

**モードの意味**:
- `editing`: ファイル編集モード
- `interactive`: 対話的モード
- `planning`: 計画モード
- `one-shot`: 一発実行モード

---

## 🚨 ツールの制約とリスク

### 認証関連

| MCP Server | 認証方法 | 必要な設定 |
|-----------|---------|-----------|
| Notion | OAuth | 事前にワークスペース接続 |
| Atlassian | OAuth | 事前にcloudId取得 |
| Akasha Memory | APIキー | `x-akasha-project`ヘッダー |
| Chrome DevTools | N/A | ブラウザインスタンス起動 |

### レート制限

- **WebSearch**: 米国でのみ利用可能
- **Notion API**: 約3 requests/sec
- **OpenAI API**: APIキーのレート制限に依存

### データサイズ制限

| ツール | 制限 |
|-------|------|
| Read | デフォルト2000行、最大文字数あり |
| Grep | `head_limit`でトランケート |
| Bash | 出力30000文字でトランケート |
| Serena tools | `max_answer_chars`パラメータ（デフォルト200000） |

### 安全性制約

**絶対禁止**:
```bash
# ❌ 破壊的コマンド
rm -rf /

# ❌ インタラクティブモード（サポート外）
git rebase -i
git add -i

# ❌ フック回避
git commit --no-verify

# ❌ 強制プッシュ（main/masterへ）
git push --force origin main
```

**Bashのサンドボックス回避**:
```typescript
Bash({
  command: "...",
  dangerouslyDisableSandbox: true  // ⚠️ 使用非推奨
})
```

### 並列実行の制約

```
✅ 独立したコマンド: 並列実行
   git status & git diff & git log

✅ 依存関係あり: &&でチェーン
   git add . && git commit -m "..." && git push

⚠️ 失敗を許容: ;で連結
   command1 ; command2 ; command3
```

---

## 💎 ベストプラクティス

### 1. ファイル操作の優先順位

```
1. Symbol-level tools
   find_symbol → replace_symbol_body

2. Edit tool
   正確な文字列置換

3. replace_regex
   ワイルドカード活用

4. Bash sed/awk
   最終手段
```

### 2. 検索の優先順位

```
1. find_symbol
   シンボル名が既知の場合

2. Grep
   コンテンツ検索

3. search_for_pattern
   複雑な正規表現パターン

4. Read
   ファイルが既知の場合

5. Task(Explore)
   広範囲な探索が必要
```

### 3. Git操作の並列化

```typescript
// ✅ 並列実行可能（情報取得のみ）
Bash("git status"),
Bash("git diff"),
Bash("git log")

// ✅ 順次実行必須（状態変更あり）
Bash("git add . && git commit -m '...' && git push")
```

### 4. エラーハンドリング

```typescript
// ✅ 失敗が許容できない
Bash("command1 && command2 && command3")

// ✅ 失敗を許容
Bash("command1 ; command2 ; command3")

// ✅ stderrキャプチャ
Bash({command: "...", capture_stderr: true})
```

### 5. Readツールの活用

```typescript
// ✅ 画像読み込み
Read({file_path: "/path/to/image.png"})

// ✅ PDF読み込み
Read({file_path: "/path/to/document.pdf"})

// ✅ Jupyter notebook読み込み
Read({file_path: "/path/to/notebook.ipynb"})

// ✅ 大きなファイルは範囲指定
Read({
  file_path: "/path/to/large.rs",
  offset: 1000,
  limit: 100
})
```

### 6. Chrome DevToolsのスナップショット優先

```
✅ Prefer: take_snapshot()
   - 高速
   - 構造化データ
   - a11yツリーベース

⚠️ Use sparingly: take_screenshot()
   - 視覚的確認が必要な場合のみ
```

---

## 🔍 未文書化の発見

### 1. Readツールのマルチメディア対応

**システムプロンプトで明記**:
```
This tool allows Claude Code to read:
- Images (PNG, JPG, etc.)
- PDF files (.pdf)
- Jupyter notebooks (.ipynb)
```

### 2. Bashのサンドボックス設定

```typescript
{
  dangerouslyDisableSandbox: true  // サンドボックス無効化
}
```

**⚠️ 警告**: セキュリティリスクあり、使用非推奨

### 3. Editツールの厳密なインデント保持

```
Preserve exact indentation (tabs/spaces) as it appears
AFTER the line number prefix
```

→ タブ/スペース混在環境での正確な編集が可能

### 4. Serenaのメタ認知機能

**発見**: コード編集前に必ず呼ぶべきthinkツール
```typescript
// 情報収集後
think_about_collected_information()

// コード編集前
think_about_task_adherence()

// 完了判定時
think_about_whether_you_are_done()
```

### 5. TodoWriteの状態機械

**FSMとして機能**:
```
- Exactly ONE task in_progress at any time
- ONLY mark completed when FULLY accomplished
- Never batch completions
```

→ タスクの完全性を厳密に保証

---

## 🌟 プロジェクト固有の発見

### 1. カスタムエージェント: task-executor

**ファイル**: `.claude/agents/task-executor.md`

```yaml
決定論的タスク実行専用エージェント
- モデル: sonnet
- ツール: Bash, mcp__github__*, mcp__*
- 用途: CI/CD待機、PRマージ、テスト実行
```

### 2. カスタムコマンド

**発見されたコマンド**:
- `/create-pr`: Pull Request作成自動化
- `/merge-pr`: Pull Requestマージ自動化

**実装**: `.claude/commands/*.md`

### 3. スキルシステム

**利用可能なスキル**:
1. **code-flow**: 開発フロー統括
   - ヒアリング → SDG → 実装 → Living Documentation

2. **spec-design-guide**: 仕様・設計管理
   - SPEC.md/DESIGN.md/GUIDE.md
   - Simplicity原則（data/calculations/actions + Straightforward）

3. **mcp-builder**: MCPサーバー構築ガイド
   - Python/Node/Rust対応
   - MCP Inspector活用

### 4. MCPサーバー設定

**ファイル**: `.mcp.json`

```json
{
  "mcpServers": {
    "serena": {
      "command": "uvx",
      "args": [
        "serena-agent",
        "serena-mcp-server",
        "--enable-web-dashboard", "true",
        "--project", "$(pwd)"
      ]
    },
    "mcp-inspector": {
      "command": "bun",
      "args": ["run", "..."]
    }
  }
}
```

**重要**: Serena Web Dashboard有効化 → リモートアクセス可能（セキュリティ注意）

### 5. 権限制御

**ファイル**: `.claude/settings.local.json`

```json
"permissions": {
  "allow": [
    "mcp__serena__check_onboarding_performed",
    "mcp__serena__activate_project",
    "mcp__serena__onboarding",
    "mcp__serena__list_dir",
    "mcp__serena__read_file",
    "mcp__serena__write_memory",
    "mcp__serena__list_memories",
    "Bash(tree:*)",
    "Bash(git:*)",
    "Bash(fleetflow:*)"  // ← 未文書化パターン
  ]
}
```

**謎のパターン**: `Bash(fleetflow:*)` → FleetFlow統合？

### 6. Hearing First手法の厳密な実装

**ファイル**: `.claude/skills/code-flow/reference/hearing-first.md`

**プロトコル**:
```typescript
// ❌ 複数質問を一度に投げる
AskUserQuestion({
  questions: [q1, q2, q3, q4]
})

// ✅ 一問一答
AskUserQuestion({questions: [q1]})
// → 回答を受け取る
AskUserQuestion({questions: [q2]})
// → 回答を受け取る
...
```

**三段階の深掘り**:
1. 第1段階: 大枠（What, Why, Who）
2. 第2段階: 詳細（How, 既存機能との関係）
3. 第3段階: 技術選択（パフォーマンス、セキュリティ）

---

## 🚀 推奨される活用アイデア

### 1. AI駆動開発環境

```
Serena (コード解析) +
Chrome DevTools (UI確認) +
Jira (タスク管理) +
Notion (ドキュメント)
= 完全自動化開発フロー
```

**実装例**:
1. Jiraからタスク取得
2. Serenaでコードベース理解
3. AskUserQuestionで要件確認
4. TodoWriteでタスク化
5. 実装 + テスト
6. Chrome DevToolsでE2E確認
7. Notionにドキュメント自動生成
8. PRを自動作成

### 2. パフォーマンス監視ダッシュボード

```typescript
// 定期実行
setInterval(async () => {
  // 1. パフォーマンス計測
  await performance_start_trace({reload: true, autoStop: true})
  const insights = await performance_analyze_insight({...})

  // 2. メトリクス抽出
  const metrics = extractMetrics(insights)

  // 3. Notionダッシュボード更新
  await notion-update-page({
    page_id: "dashboard",
    data: {
      command: "replace_content",
      new_str: generateReport(metrics)
    }
  })
}, 3600000)  // 1時間ごと
```

### 3. ナレッジベース自動構築

```typescript
// 1. 全シンボル抽出
const symbols = await find_symbol({
  name_path: "/",
  depth: 3,
  include_body: true
})

// 2. Notionデータベース作成
const db = await notion-create-database({
  properties: {
    "Name": {type: "title"},
    "Type": {type: "select", select: {options: [...]}},
    "File": {type: "rich_text"},
    "Documentation": {type: "rich_text"}
  }
})

// 3. ページ一括作成
await notion-create-pages({
  parent: {database_id: db.id},
  pages: symbols.map(sym => ({
    properties: {
      "Name": sym.name,
      "Type": sym.kind,
      "File": sym.file,
      "Documentation": sym.docstring
    }
  }))
})

// 4. 定期同期
// コード変更時にNotionを自動更新
```

### 4. クロスプラットフォームイシュートラッキング

```typescript
// Jira ⇄ GitHub ⇄ Notion 双方向同期

// GitHub issue → Jira
const ghIssues = await Bash("gh issue list --json number,title,body")
for (const issue of ghIssues) {
  await createJiraIssue({
    cloudId: "...",
    projectKey: "PROJ",
    issueTypeName: "Task",
    summary: issue.title,
    description: issue.body
  })
}

// Jira → Notion
const jiraIssues = await searchJiraIssuesUsingJql({
  cloudId: "...",
  jql: "project = PROJ"
})
for (const issue of jiraIssues) {
  await notion-create-pages({
    parent: {database_id: "..."},
    pages: [{
      properties: {
        "Title": issue.fields.summary,
        "Status": issue.fields.status.name,
        "Assignee": issue.fields.assignee.displayName
      }
    }]
  })
}
```

### 5. 自動コードレビューシステム

```typescript
// 1. PRの変更を取得
const diff = await Bash("git diff main...HEAD")

// 2. Serenaで変更シンボルを特定
const changedSymbols = await find_referencing_symbols({...})

// 3. メタ認知ツールで評価
await think_about_collected_information()
await think_about_task_adherence()

// 4. Chrome DevToolsでE2Eテスト
await new_page({url: "http://localhost:3000"})
await fill_form([...])
await click({uid: "submit"})
await wait_for({text: "Success"})

// 5. レビューコメント自動生成
const review = generateReviewComment(changedSymbols, testResults)

// 6. GitHubにコメント投稿
await Bash(`gh pr comment ${prNumber} --body "${review}"`)
```

---

## 📊 ツール使い分けマトリックス

| 状況 | 推奨ツール | 理由 |
|------|----------|------|
| ユーザーに質問 | AskUserQuestion | 対話的情報収集 |
| 複雑なタスク | TodoWrite + Task | 状態管理 + 専門エージェント |
| 最新情報取得 | WebSearch | 知識カットオフ以降 |
| ドキュメント取得 | WebFetch | 静的コンテンツ |
| コードベース探索 | Task(Explore) | 広範囲な調査 |
| シンボル検索 | find_symbol | 名前が既知 |
| コンテンツ検索 | Grep | パターンマッチ |
| ファイル編集 | Edit | 正確な置換 |
| シンボル編集 | replace_symbol_body | クラス/関数単位 |
| 長時間実行 | Bash(background) | ビルド、サーバー起動 |
| ブラウザ操作 | Chrome DevTools | E2Eテスト、UI確認 |
| ドキュメント管理 | Notion MCP | 知識ベース構築 |
| タスク管理 | Jira MCP | イシュー追跡 |
| メモリ保存 | Serena write_memory | プロジェクトコンテキスト |

---

## 🎯 まとめ

### 主要な発見

1. **Chrome DevTools MCP**: 完全なブラウザ自動化スイート
2. **Notion/Atlassian MCP**: エンタープライズ統合
3. **Serenaメタ認知機能**: AIの自己評価メカニズム
4. **TodoWrite FSM**: タスクの完全性保証
5. **Git Safety Protocol**: 厳格な安全性制約
6. **Hearing First一問一答**: 段階的要件明確化

### 重要な制約

- MCP統合はOAuth事前設定必須
- バックグラウンド実行は10分制限
- WebSearchは米国のみ
- 並列実行時の依存関係管理が重要

### セキュリティリスク

- `dangerouslyDisableSandbox`の使用は避ける
- 破壊的Bashコマンドは禁止
- 認証情報の漏洩に注意（`_meta`フィールド）

### 次のステップ

これらのツールを組み合わせることで、**AI駆動の完全自動化開発環境**が構築可能：

```
Hearing First（要件明確化）
    ↓
SDG（仕様・設計）
    ↓
Serena（コード解析・編集）
    ↓
Chrome DevTools（E2Eテスト）
    ↓
Notion/Jira（ドキュメント・タスク管理）
    ↓
Git/GitHub（バージョン管理・PR）
    ↓
Living Documentation（継続的同期）
```

**特にSerenaのメタ認知機能とTodoWriteのFSMを組み合わせることで、自己修正可能なAIエージェントが実現できる。**

---

**調査日**: 2025-11-18
**調査者**: 並列起動された凄腕ハッカーエージェント（general-purpose + Explore）
**調査範囲**: システムプロンプト全体 + プロジェクト内全ファイル（very thorough）
