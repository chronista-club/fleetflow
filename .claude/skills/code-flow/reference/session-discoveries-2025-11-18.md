# Claude Code内部調査セッション総合レポート

**日付**: 2025-11-18
**調査者**: Claude (凄腕ハッカーモード 😎)
**手法**: 並列エージェント調査 + 実験的検証

---

## 🎯 セッション概要

このセッションでは、Claude Codeの内部ツールとMCPサーバー統合を徹底的に調査しました。並列エージェントを活用した「ハッカー調査」により、**90個以上のツール**を発見し、その仕様と使用方法を完全にドキュメント化しました。

### 達成したこと

1. ✅ AskUserQuestionツールの完全仕様を解明・文書化
2. ✅ Claude Code内部ツール10個の詳細リファレンス作成
3. ✅ MCP統合ツール80+個の発見と仕様書作成
4. ✅ Serenaメタ認知ツールの実験的検証
5. ✅ ブレイン統合設計の提案
6. ✅ Code Flowスキルの完成形構築

---

## 📚 作成ドキュメント一覧

### 1. [AskUserQuestionツール完全仕様](ask-user-question-tool.md)

**内容**:
- JSONSchema定義
- 制約事項（質問数1-4、オプション数2-4、header最大12文字）
- 自動「Other」オプション追加の発見
- multiSelectによる単一/複数選択制御
- Hearing First手法との統合
- 3つの実用的使用例
- ベストプラクティス

**重要な発見**:
```typescript
// 質問は1-4個、オプションは2-4個
// headerは最大12文字
// 自動的に「Other」オプションが追加される
AskUserQuestion({
  questions: [{
    question: "認証方式はどれを使いますか？",
    header: "認証方式",  // 12文字以内
    multiSelect: false,  // 単一選択
    options: [
      {label: "JWT", description: "トークンベース認証"},
      {label: "OAuth", description: "外部プロバイダー連携"}
    ]
  }]
})
```

### 2. [Claude Code内部ツール一覧](claude-code-internal-tools.md)

**10個の主要内部ツール**:

1. **Task** - サブエージェント起動（general-purpose, Explore, Plan, task-executor）
2. **TodoWrite** - タスク管理FSM（pending/in_progress/completed）
3. **AskUserQuestion** - 対話的質問
4. **ExitPlanMode** - プランモード制御
5. **Skill** - スキル実行
6. **SlashCommand** - カスタムコマンド
7. **WebFetch** - Webコンテンツ取得（15分キャッシュ）
8. **WebSearch** - Web検索（米国のみ）
9. **BashOutput/KillShell** - バックグラウンドシェル管理
10. **ListMcpResourcesTool/ReadMcpResourceTool** - MCPリソース管理

**各ツールの内容**:
- JSONSchema仕様
- 実践的使用例
- ベストプラクティス
- 使い分けガイド
- 組み合わせパターン

### 3. [高度な発見レポート](claude-code-advanced-discoveries.md)

**80+個のMCPツール発見**:

#### Chrome DevTools MCP (30+ツール)
```typescript
// ブラウザ自動化完全スイート
mcp__chrome-devtools__navigate_page({url: "https://example.com"})
mcp__chrome-devtools__click({uid: "button-123"})
mcp__chrome-devtools__fill({uid: "input-456", value: "test"})
mcp__chrome-devtools__take_screenshot({fullPage: true})
mcp__chrome-devtools__performance_start_trace({reload: true})
mcp__chrome-devtools__list_network_requests()
```

**用途**: E2Eテスト自動生成、パフォーマンス診断、ネットワーク監視

#### Notion MCP (20+ツール)
```typescript
// エンタープライズドキュメント管理
mcp__notion__notion-search({query: "API設計", query_type: "internal"})
mcp__notion__notion-fetch({id: "page-id"})
mcp__notion__notion-create-pages({pages: [{...}]})
mcp__notion__notion-update-page({page_id: "...", command: "replace_content"})
mcp__notion__notion-list-agents()  // カスタムエージェント一覧
```

**特徴**: Notion-flavored Markdown、カスタムエージェント統合

#### Atlassian MCP (25+ツール)
```typescript
// Confluence/Jira完全統合
mcp__atlassian__searchConfluenceUsingCql({cql: "title ~ 'API' AND type = page"})
mcp__atlassian__searchJiraIssuesUsingJql({jql: "project = PROJ AND status = Open"})
mcp__atlassian__createJiraIssue({projectKey: "PROJ", issueTypeName: "Task"})
mcp__atlassian__createConfluencePage({spaceId: "...", body: "..."})
```

**用途**: ドキュメント管理、課題追跡、自動化ワークフロー

#### Serena MCP (25+ツール) - **メタ認知機能の発見！**
```typescript
// コード解析特化
mcp__serena__find_symbol({name_path: "Cli", include_body: true})
mcp__serena__find_referencing_symbols({name_path: "function_name"})
mcp__serena__replace_symbol_body({...})
mcp__serena__search_for_pattern({substring_pattern: "regex_pattern"})

// 🧠 メタ認知ツール（AIの自己評価）
mcp__serena__think_about_collected_information()
mcp__serena__think_about_task_adherence()
mcp__serena__think_about_whether_you_are_done()

// メモリシステム
mcp__serena__list_memories()
mcp__serena__write_memory({memory_name: "...", content: "..."})
mcp__serena__read_memory({memory_file_name: "..."})
```

**重要な発見**: メタ認知ツールはAIに自己反省プロンプトを返す！

#### Akasha Memory MCP
```typescript
// クロスプラットフォームメモリ
mcp__akasha-memory__addMemory({thingToRemember: "..."})
mcp__akasha-memory__search({informationToGet: "..."})
mcp__akasha-memory__whoAmI()
```

### 4. [ブレイン統合設計](brain-integration.md)

**永続的な構成可能なブレインシステム**の設計提案:

#### アーキテクチャ

```typescript
interface Brain {
  memoryBank: MemoryBank,
  patternRepository: PatternRepository,
  learningEngine: LearningEngine,
  contextEngine: ContextEngine
}

interface MemoryBank {
  projects: {[projectId: string]: {
    overview: string,
    architecture: string,
    patterns: Pattern[],
    decisions: Decision[]
  }},
  global: {
    bestPractices: BestPractice[],
    commonPatterns: Pattern[],
    lessonLearned: Lesson[]
  }
}

interface Pattern {
  id: string,
  name: string,
  category: "authentication" | "api" | "database" | ...,
  hearingQuestions: AskUserQuestion[],
  specTemplate: string,
  designTemplate: string,
  codeExamples: CodeExample[],
  checklistTemplate: ChecklistItem[],
  usedCount: number,
  successRate: number,
  lastUsed: Date
}
```

#### 5フェーズフロー（Brain統合版）

```
Phase 0: Brain Consultation
  → パターンマッチング
  → 類似ケース検索
  ↓
Phase 1: Hearing First（最適化版）
  → Brainからの質問テンプレート
  → 段階的深掘り
  ↓
Phase 2: SDG（テンプレート駆動）
  → パターンからSPEC/DESIGN生成
  → カスタマイズ
  ↓
Phase 3: Implementation
  → パターンのコード例活用
  → チェックリスト駆動
  ↓
Phase 4: Living Documentation
  → 同期確認・コミット
  ↓
Phase 5: Learning & Feedback
  → パターン抽出・更新
  → 成功率記録
```

#### 期待効果

| 指標 | 現状 | Brain導入後 |
|------|------|-------------|
| ヒアリング時間 | 10-15分 | 3-5分 |
| SPEC.md作成 | 15-20分 | 5-10分 |
| DESIGN.md作成 | 20-30分 | 10-15分 |
| パターン再利用率 | 0% | 70% |
| 手戻り発生率 | 15% | 5% |

#### 実装ロードマップ

- **Phase 1** (1-2週間): 基盤構築（Memory Bank, Pattern Repository）
- **Phase 2** (2-3週間): パターン機能（CRUD, Learning Engine v1）
- **Phase 3** (3-4週間): 学習機能（自動抽出、最適化）
- **Phase 4** (継続): 最適化・拡張

---

## 🔬 実験的検証

### Experiment 1: Serenaメタ認知ツール

```bash
$ mcp__serena__think_about_collected_information()
```

**結果**: 自己反省プロンプトを返す
```
"Have you collected all the information you need for this task?
Consider whether you should read more code or if you can proceed..."
```

**発見**: これはAIが自己評価するためのメカニズム！TodoWriteのFSMと組み合わせることで、タスク完了判断を厳密化できる。

### Experiment 2: メモリシステム

```bash
$ mcp__serena__list_memories()
```

**結果**: 8個の既存メモリを発見
- `codebase_structure`
- `project_overview`
- `development_patterns`
- 他5個

### Experiment 3: メモリ書き込み

```bash
$ mcp__serena__write_memory({
  memory_name: "claude_code_internal_tools_discoveries",
  content: "# 完全調査結果..."
})
```

**結果**: 成功 - 永続的メモリとして保存された

### Experiment 4-5: シンボル検索

```bash
$ mcp__serena__get_symbols_overview({
  relative_path: "crates/akasha-mcp/src/main.rs"
})
```

**結果**:
- モジュール: auth, http, logging, memory, metrics, server, tools
- 構造体: Cli
- 列挙型: TransportMode
- 関数: main

```bash
$ mcp__serena__find_symbol({
  name_path: "Cli",
  include_body: true
})
```

**結果**: Cli構造体の完全な定義を取得（フィールド、アノテーション、位置情報）

---

## 💡 主要な発見

### 1. TodoWrite FSM（有限状態機械）

```
pending → in_progress → completed

制約:
- 同時に in_progress は1つのみ
- 完全に達成した時のみ completed
- 失敗/ブロック時は in_progress を維持
```

### 2. Git Safety Protocol

```
❌ NEVER:
- update git config
- push --force (main/masterには絶対に)
- skip hooks (--no-verify, --no-gpg-sign)
- commit --amend (明示的要求時のみ)

✅ ALWAYS:
- authorship確認 (git log -1 --format='%an %ae')
- HEREDOCでコミットメッセージ
- 秘密情報の検査 (.env, credentials.json等)
```

### 3. 未文書化機能

- 📷 **画像読み込み**: ReadツールでPNG/JPG読み込み可能
- 📄 **PDF読み込み**: ReadツールでPDF読み込み可能
- 🔄 **バックグラウンド実行**: Bash `run_in_background` パラメータ
- 📊 **Jupyter Notebook**: NotebookEditツールの存在

### 4. Serenaメタ認知ツールの意義

AIが「自分が何を知っているか」「タスクを完了したか」を自己評価するメカニズム。
これにより：
- ✅ 不完全な実装の防止
- ✅ タスク逸脱の検出
- ✅ 情報収集の最適化

が可能になる。

---

## 🚀 高度な使用パターン

### パターン1: E2Eテスト自動生成

```typescript
// 1. ページをナビゲート
mcp__chrome-devtools__navigate_page({url: "https://app.example.com"})

// 2. スナップショット取得（要素を確認）
mcp__chrome-devtools__take_snapshot()

// 3. 操作実行
mcp__chrome-devtools__fill({uid: "email-input", value: "test@example.com"})
mcp__chrome-devtools__click({uid: "login-button"})

// 4. 結果検証
mcp__chrome-devtools__wait_for({text: "ダッシュボード"})
mcp__chrome-devtools__take_screenshot({filePath: "./test-result.png"})

// 5. ネットワーク確認
mcp__chrome-devtools__list_network_requests({
  resourceTypes: ["xhr", "fetch"]
})
```

### パターン2: パフォーマンス診断フロー

```typescript
// 1. トレース開始
mcp__chrome-devtools__performance_start_trace({reload: true, autoStop: false})

// 2. 操作実行
// ... user interactions ...

// 3. トレース停止・分析
mcp__chrome-devtools__performance_stop_trace()

// 4. インサイト取得
mcp__chrome-devtools__performance_analyze_insight({
  insightSetId: "...",
  insightName: "LCPBreakdown"
})
```

### パターン3: Jira-GitHub統合ワークフロー

```typescript
// 1. Jira課題検索
const issues = await mcp__atlassian__searchJiraIssuesUsingJql({
  jql: "project = PROJ AND status = 'In Progress'"
})

// 2. 各課題に対してブランチ作成・実装
for (const issue of issues) {
  // Git操作
  await Bash({command: `git checkout -b feature/${issue.key}`})

  // 実装...

  // コミット
  await Bash({command: `git commit -m "${issue.fields.summary}"`})

  // Jira更新
  await mcp__atlassian__transitionJiraIssue({
    issueIdOrKey: issue.key,
    transition: {id: "done-transition-id"}
  })
}
```

### パターン4: 自動ドキュメント生成パイプライン

```typescript
// 1. コード解析（Serena）
const symbols = await mcp__serena__get_symbols_overview({
  relative_path: "src/api/handlers.rs"
})

// 2. API仕様抽出
const endpoints = symbols.filter(s => s.kind === 12) // functions

// 3. Notion/Confluenceドキュメント生成
for (const endpoint of endpoints) {
  await mcp__notion__notion-create-pages({
    parent: {page_id: "api-docs-page"},
    pages: [{
      properties: {title: endpoint.name},
      content: generateApiDoc(endpoint)
    }]
  })
}

// 4. Jiraと紐付け
await mcp__atlassian__addCommentToJiraIssue({
  issueIdOrKey: "PROJ-123",
  commentBody: `API仕様書: [${endpoint.name}](notion-url)`
})
```

### パターン5: クロスプラットフォームメモリ同期

```typescript
// Serena → Akasha Memory
const memories = await mcp__serena__list_memories()
for (const memory of memories) {
  const content = await mcp__serena__read_memory({memory_file_name: memory})
  await mcp__akasha-memory__addMemory({thingToRemember: content})
}

// Akasha Memory → Notion
const query = await mcp__akasha-memory__search({
  informationToGet: "認証パターン"
})
await mcp__notion__notion-create-pages({
  pages: [{
    properties: {title: "認証パターン"},
    content: query
  }]
})
```

---

## 📊 組み合わせの威力

### Hearing First × Brain × Notion

```
1. Brain: 類似パターン検索
   ↓
2. AskUserQuestion: 最適化された質問
   ↓
3. SPEC.md生成: パターンテンプレート活用
   ↓
4. Notion: ドキュメント自動作成
   ↓
5. Brain: パターン学習・更新
```

### CI/CD × Chrome DevTools × Jira

```
1. Git push
   ↓
2. E2Eテスト実行（Chrome DevTools）
   ↓
3. パフォーマンス診断
   ↓
4. 結果をJiraに自動投稿
   ↓
5. 失敗時はブランチをブロック
```

---

## 🎯 今後の展開

### 短期（1-2週間）
- [x] 内部ツール完全調査 ✅
- [x] リファレンスドキュメント作成 ✅
- [x] ブレイン統合設計 ✅
- [ ] ブレイン実装 Phase 1開始
- [ ] パターンリポジトリのプロトタイプ

### 中期（1-2ヶ月）
- [ ] ブレイン実装完了
- [ ] 実プロジェクトでの検証
- [ ] パターンライブラリの充実
- [ ] Chrome DevTools統合（E2Eテスト自動生成）
- [ ] Notion/Atlassian統合（ドキュメント自動化）

### 長期（3-6ヶ月）
- [ ] マルチプロジェクト対応
- [ ] チーム共有パターンリポジトリ
- [ ] AI自律エージェント化
- [ ] クラウド環境での展開

---

## 📝 メタデータ

**作成日**: 2025-11-18
**調査手法**: 並列エージェント（general-purpose × 2, Explore very thorough × 1）
**検証手法**: 実験的ツール実行（5実験）
**発見ツール数**: 90+個
**作成ドキュメント数**: 6個
**コミット**: 3回（72e4060d, 8b8fceee, 515814f4）

**関連ドキュメント**:
- [AskUserQuestionツール完全仕様](ask-user-question-tool.md)
- [Claude Code内部ツール一覧](claude-code-internal-tools.md)
- [高度な発見レポート](claude-code-advanced-discoveries.md)
- [ブレイン統合設計](brain-integration.md)
- [開発フロー](development-flow.md)
- [ヒアリングファースト手法](hearing-first.md)

---

**調査完了。開発を楽しみましょう！😊🚀**
