---
skill: code-flow
description: ヒアリングファーストで要件を明確化し、SDGで仕様・設計を記録する開発フロー
tags: [development, workflow, sdg, brain]
version: 1.0.0
---

# Code Flow Skill

**Code Flow**は、Brain機能を活用した5フェーズ開発ワークフローを提供します。

## 概要

Code Flowは以下の5つのフェーズで構成されています:

```
Phase 1: Brain相談 → Phase 2: ヒアリング → Phase 3: SDG → Phase 4: 実装 → Phase 5: 学習
```

### Phase 1: Brain相談
ユーザーリクエストから関連するパターンを推奨します。

### Phase 2: ヒアリング
選択したパターンに基づいて質問を生成し、ユーザーから回答を収集します。

### Phase 3: SDG（Spec-Design-Guide）
収集した情報を元に、SPEC.md（仕様書）とDESIGN.md（設計書）を生成します。

### Phase 4: 実装
チェックリストを生成し、実装をガイドします。

### Phase 5: 学習
セッションを記録し、パターンの成功率を更新。将来的なパターン発見に活用します。

## 使い方

### 1. Code Flowの実行

```bash
# パターンをロード
cd packages/akasha-brain
bun run src/pattern-repository/load-seeds.ts

# Code Flowを実行
bun run src/workflows/code-flow.ts "ユーザーリクエスト" "プロジェクトID"

# 例
bun run src/workflows/code-flow.ts "REST APIに認証機能を追加したい" "my-project"
```

### 2. TypeScriptから使用

```typescript
import { Brain } from '@akasha/brain';
import { CodeFlow } from '@akasha/brain/workflows/code-flow';

const brain = await Brain.create();
const codeFlow = new CodeFlow(brain);

const context = await codeFlow.run(
  'REST APIに認証機能を追加したい',
  'my-project'
);

console.log('選択パターン:', context.selectedPattern?.name);
console.log('SPEC:', context.spec);
console.log('DESIGN:', context.design);
console.log('チェックリスト:', context.checklist);
```

### 3. MCPツールから使用

MCP ServerがBrain機能を提供している場合、以下のツールを使用できます:

#### Phase 1: Brain相談
```json
{
  "name": "consultBrain",
  "arguments": {
    "userRequest": "REST APIに認証機能を追加したい",
    "projectId": "my-project"
  }
}
```

#### Phase 1: パターン推奨
```json
{
  "name": "recommendPatterns",
  "arguments": {
    "userRequest": "REST APIに認証機能を追加したい",
    "limit": 5
  }
}
```

#### Phase 2: 質問生成
```json
{
  "name": "generateQuestions",
  "arguments": {
    "patternId": "<pattern-id>"
  }
}
```

#### Phase 3: テンプレート生成
```json
{
  "name": "generateTemplates",
  "arguments": {
    "patternId": "<pattern-id>",
    "userAnswers": {
      "認証方式": "APIキー",
      "スコープ": "プロジェクト単位"
    }
  }
}
```

#### Phase 4: チェックリスト生成
```json
{
  "name": "generateChecklist",
  "arguments": {
    "patternId": "<pattern-id>"
  }
}
```

#### Phase 5: セッション記録
```json
{
  "name": "recordSession",
  "arguments": {
    "userRequest": "REST APIに認証機能を追加したい",
    "patternId": "<pattern-id>",
    "spec": "...",
    "design": "...",
    "implementation": "...",
    "success": true,
    "testsPassed": true
  }
}
```

## パターン管理

### 利用可能なパターン

現在のパターンリポジトリには以下のカテゴリのパターンがあります:

- **authentication**: 認証関連パターン（APIキー、JWT、OAuth2等）
- **api-design**: API設計パターン（RESTful CRUD等）
- **data-model**: データモデル設計パターン（TypeScript型定義等）
- **testing**: テストパターン（Bun.js unit/integration test等）
- **deployment**: デプロイメントパターン（Docker containerization等）
- **security**: セキュリティパターン（Input validation, XSS prevention等）

### 新しいパターンの追加

```json
{
  "name": "savePattern",
  "arguments": {
    "name": "新しいパターン",
    "category": "authentication",
    "description": "パターンの説明",
    "specTemplate": "# SPEC.md テンプレート\n...",
    "designTemplate": "# DESIGN.md テンプレート\n...",
    "tags": ["auth", "security"]
  }
}
```

### パターン発見

セッション履歴から新しいパターンを自動発見します:

```json
{
  "name": "discoverPatterns",
  "arguments": {
    "minSessions": 2
  }
}
```

## SDG原則

Code Flowは**SDG（Spec-Design-Guide）原則**に基づいています:

### SPEC.md（仕様書） - What & Why
- **何を**実装するのか
- **なぜ**それが必要なのか
- ユーザー視点での価値

### DESIGN.md（設計書） - How
- **どのように**実装するのか
- アーキテクチャとコンポーネント設計
- 技術選択とその理由

### Living Documentation
- コードと同期して更新
- 実装と設計の乖離を防ぐ
- セッション記録で継続的に改善

## ベストプラクティス

1. **ヒアリングファースト**
   - パターン選択後、必ず質問を通じてコンテキストを収集
   - ユーザーの意図を正確に理解

2. **SDG準拠**
   - SPEC.mdとDESIGN.mdを必ず作成
   - Why（なぜ）とHow（どのように）を明確に分離

3. **チェックリスト活用**
   - 実装前にチェックリストを確認
   - 抜け漏れを防ぐ

4. **継続的学習**
   - すべてのセッションを記録
   - 成功・失敗から学習してパターン改善

5. **パターン発見**
   - 定期的にdiscoverPatternsを実行
   - 自動発見されたパターンをレビューして精緻化

## トラブルシューティング

### パターンが見つからない

```bash
# パターンをロード
cd packages/akasha-brain
bun run src/pattern-repository/load-seeds.ts
```

### Brain機能が動作しない

```bash
# Brainテストを実行
mise run test:brain

# 全テストを実行
bun test
```

### MCP Serverでツールが見つからない

```bash
# MCP Server起動確認
cd apps/akasha-mcp
bun run src/index.ts

# ログ確認
cat /tmp/mcp-server.log
```

## 関連ドキュメント

### プロジェクトドキュメント
- [Brain README](../../packages/akasha-brain/README.md) - Brain機能の詳細
- [SPEC.md](../../SPEC.md) - プロジェクト仕様書
- [DESIGN.md](../../DESIGN.md) - プロジェクト設計書
- [Code Flow Demo](../../packages/akasha-brain/src/demos/brain-flow-demo.ts) - デモスクリプト

### Code Flowリファレンス
- [Brain Integration](./reference/brain-integration.md) - Brain統合の全体像
- [Development Flow](./reference/development-flow.md) - 開発フロー詳細
- [Hearing First](./reference/hearing-first.md) - ヒアリングファースト手法
- [AskUserQuestion Tool](./reference/ask-user-question-tool.md) - 質問ツールの使い方
- [Claude Code Advanced Discoveries](./reference/claude-code-advanced-discoveries.md) - 高度な機能の発見
- [Claude Code Internal Tools](./reference/claude-code-internal-tools.md) - 内部ツールの詳細
- [Session Discoveries 2025-11-18](./reference/session-discoveries-2025-11-18.md) - セッション記録
