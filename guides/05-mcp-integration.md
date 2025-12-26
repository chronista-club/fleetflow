# Guide: AI エージェント連携 (Gemini CLI / Claude Code)

FleetFlow は MCP (Model Context Protocol) をサポートしており、Gemini CLI や Claude Code といった AI エージェントから直接操作することができます。

## 1. 事前準備

FleetFlow がインストールされており、パスが通っていることを確認してください。

```bash
flow --version
```

## 2. Gemini CLI での設定

Gemini CLI を使用している場合は、プロジェクトディレクトリの `.gemini/settings.json` に以下の設定を追加します。

```json
{
  "mcpServers": {
    "fleetflow": {
      "displayName": "FleetFlow",
      "command": "fleetflow",
      "args": ["mcp"],
      "type": "stdio"
    }
  }
}
```

## 3. Claude Code での設定

Claude Code (anthropic/claude-code) を使用している場合は、以下のコマンドで MCP サーバーとして登録できます。

```bash
claude mcp add fleetflow -- flow mcp
```

または、グローバル設定ファイル（通常は `~/.claude/config.json` または `~/.config/claude/config.json`）に直接記述することも可能です。

```json
{
  "mcpServers": {
    "fleetflow": {
      "command": "fleetflow",
      "args": ["mcp"]
    }
  }
}
```

## 4. AI との対話例

連携が完了すると、AI に対して以下のような自然言語での指示が可能になります。

- 「今のプロジェクトの構成を教えて」
- 「開発環境（localステージ）を起動して」
- 「コンテナの稼働状況を確認して」
- 「エラーが出ていないか ps で確認して、必要ならログを見せて」

## 5. 教育的なポイント（大学生へのアドバイス）

- **MCP は AI の「手足」**: AI が単にテキストを生成するだけでなく、現実のシステム（Docker 等）を操作するための共通プロトコルであることを理解しましょう。
- **宣言的操作**: 自分で `docker ps` を打つ代わりに、AI に「状態を見て」と頼むことで、人間は「何を実現したいか」という高レベルな設計に集中できるようになります。
- **FleetFlow の正式名称**: 常に **FleetFlow** (FとFが大文字) と呼びましょう。これは「群（Fleet）」を「流す（Flow）」という意図を忘れないためです。
