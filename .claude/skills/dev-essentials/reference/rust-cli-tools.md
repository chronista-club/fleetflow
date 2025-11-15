# Rust CLI Tools リファレンス

## 概要

Rustで実装された高速で便利なCLIツール群のリファレンスです。従来のUnixツールの代替として、より高速で使いやすい機能を提供します。

---

## lsd (LSDeluxe)

`ls`コマンドの現代的な代替ツール。カラフルな出力とアイコン表示で視認性を向上。

### インストール

```bash
# Homebrew (macOS/Linux)
brew install lsd

# Cargo
cargo install lsd

# apt (Debian/Ubuntu)
sudo apt install lsd
```

### 基本的な使い方

```bash
# 基本的なリスト表示
lsd

# 詳細表示
lsd -l
lsd --long

# 全てのファイルを表示（隠しファイル含む）
lsd -a
lsd --all

# ツリー表示
lsd --tree
lsd --tree --depth 2

# サイズでソート
lsd -lS
lsd --long --size-sort

# 時間でソート
lsd -lt
lsd --long --timesort
```

### よく使うオプション

```bash
# 人間が読みやすいサイズ表示
lsd -lh

# アイコンなし（高速化）
lsd --icon never

# ディレクトリのみ表示
lsd -d */

# ファイルタイプ別に色分け
lsd --color always

# タイムスタンプの相対表示
lsd -l --date relative

# グリッド表示
lsd --grid

# 再帰的に表示
lsd -R
lsd --tree
```

### 設定ファイル

`~/.config/lsd/config.yaml`

```yaml
# デフォルト設定
classic: false
blocks:
  - permission
  - user
  - group
  - size
  - date
  - name
color:
  when: auto
date: date
dereference: false
display: default
icons:
  when: auto
  theme: fancy
  separator: " "
ignore-globs:
  - .git
  - .DS_Store
indicators: false
layout: grid
recursion:
  enabled: false
  depth: 1
size: default
permission: rwx
sorting:
  column: name
  reverse: false
  dir-grouping: none
no-symlink: false
total-size: false
hyperlink: never
header: false
```

### エイリアス設定例

```bash
# .bashrc または .zshrc
alias ls='lsd'
alias ll='lsd -l'
alias la='lsd -a'
alias lt='lsd --tree'
alias l='lsd -lah'
```

### よくあるユースケース

```bash
# プロジェクトディレクトリの構造を確認
lsd --tree --depth 3 --ignore-glob node_modules

# 最近変更されたファイルを表示
lsd -lt | head -n 10

# サイズの大きいファイルを探す
lsd -lSr

# gitリポジトリの状態を確認
lsd -la

# ディレクトリサイズの確認
lsd -l --total-size

# シンボリックリンクの確認
lsd -l | grep '^l'
```

### lsdの特徴

- ✅ **高速**: Rust実装による高いパフォーマンス
- ✅ **視認性**: カラフルな出力とアイコン表示
- ✅ **互換性**: `ls`コマンドのオプションをほぼサポート
- ✅ **カスタマイズ**: YAMLによる柔軟な設定
- ✅ **Git統合**: Git管理ファイルの状態を表示可能

---

## その他の便利なRust CLIツール

### bat - `cat`の代替

```bash
# インストール
brew install bat

# 使い方
bat file.rs              # シンタックスハイライト付き表示
bat -n file.rs          # 行番号付き
bat -A file.rs          # 非表示文字も表示
bat file1.rs file2.rs   # 複数ファイル

# エイリアス
alias cat='bat'
```

### ripgrep (rg) - `grep`の代替

```bash
# インストール
brew install ripgrep

# 使い方
rg "pattern" .                    # 高速検索
rg -i "pattern" .                 # 大文字小文字無視
rg -t rust "pattern" .            # ファイルタイプ指定
rg --hidden "pattern" .           # 隠しファイルも検索
rg -l "pattern" .                 # ファイル名のみ表示
```

### fd - `find`の代替

```bash
# インストール
brew install fd

# 使い方
fd pattern                        # シンプルな検索
fd -e rs                          # 拡張子指定
fd -t f pattern                   # ファイルのみ
fd -t d pattern                   # ディレクトリのみ
fd -H pattern                     # 隠しファイルも含む
```

### exa - `ls`の代替（lsdの代替候補）

```bash
# インストール
brew install exa

# 使い方
exa                               # 基本表示
exa -l                            # 詳細表示
exa --tree                        # ツリー表示
exa --git                         # Git情報付き
```

### zoxide - `cd`の改善

```bash
# インストール
brew install zoxide

# シェル統合
echo 'eval "$(zoxide init bash)"' >> ~/.bashrc
echo 'eval "$(zoxide init zsh)"' >> ~/.zshrc

# 使い方
z project                         # ディレクトリ名で移動
zi project                        # インタラクティブ選択
```

### tokei - コード行数カウント

```bash
# インストール
brew install tokei

# 使い方
tokei                             # プロジェクト全体
tokei src/                        # 特定ディレクトリ
tokei --sort lines                # 行数でソート
```

### hyperfine - ベンチマーク

```bash
# インストール
brew install hyperfine

# 使い方
hyperfine 'command1' 'command2'   # コマンド比較
hyperfine -w 3 'command'          # ウォームアップ3回
hyperfine --export-markdown results.md 'cmd' # 結果をMarkdownに
```

### dust - `du`の代替

```bash
# インストール
brew install dust

# 使い方
dust                              # ディレクトリサイズ表示
dust -d 2                         # 深さ制限
dust -r                           # 逆順ソート
```

---

## Rust CLIツールのベストプラクティス

### 1. エイリアス設定

```bash
# ~/.bashrc または ~/.zshrc
# モダンツールへの置き換え
alias ls='lsd'
alias cat='bat'
alias find='fd'
alias grep='rg'
```

### 2. 設定ファイルの管理

```bash
# dotfiles管理
mkdir -p ~/.config/{lsd,bat}
ln -s ~/dotfiles/.config/lsd/config.yaml ~/.config/lsd/config.yaml
```

### 3. シェル統合

```bash
# 補完の有効化
eval "$(zoxide init bash)"
source <(lsd --generate-completion bash)
```

### 4. パフォーマンス最適化

```bash
# ディレクトリサイズの確認にdustを使用
dust -d 2 ~/projects

# 大規模検索にripgrepを使用
rg --threads 8 "pattern" /large/directory
```

---

## トラブルシューティング

### lsdでアイコンが表示されない

```bash
# Nerd Fontをインストール
brew tap homebrew/cask-fonts
brew install --cask font-hack-nerd-font

# ターミナルのフォント設定を変更
```

### パフォーマンスが遅い

```bash
# アイコンを無効化
lsd --icon never

# 設定ファイルでデフォルト無効化
echo "icons:
  when: never" > ~/.config/lsd/config.yaml
```

### エイリアスが機能しない

```bash
# シェル設定の再読み込み
source ~/.bashrc  # または ~/.zshrc

# エイリアスの確認
alias ls
```

---

## 参考リンク

- [lsd GitHub](https://github.com/lsd-rs/lsd)
- [Rust CLI Tools Collection](https://lib.rs/command-line-utilities)
- [Modern Unix Tools](https://github.com/ibraheemdev/modern-unix)
