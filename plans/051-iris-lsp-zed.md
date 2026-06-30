---
name: 051-iris-lsp-zed
description: iris-lang 用 LSP サーバーを新規クレートで実装し、Zed エディタで `.iris` ファイルの診断を表示する
commit: 5b6c186
status: TODO
---

# Plan 051 — iris-lsp: LSP サーバー + Zed インテグレーション

## 背景と目標

iris-lang コンパイラが返すエラー（字句・構文・名前解決・型・所有権）を Zed エディタに
インラインで表示したい。実装は最小限（diagnostics のみ。補完・ホバーは対象外）。

**やること:**
1. `src/lib.rs` に LSP 向け診断収集関数を追加（miette なし・構造化エラーのみ）
2. 新しい Cargo ワークスペースメンバ `iris-lsp/` に LSP サーバーバイナリを作る
3. `cargo install` でバイナリをインストール
4. Zed の `settings.json` で `.iris` ファイルとバイナリを紐付ける

**やらないこと（将来のプランへ）:**
- 補完、ホバー、定義ジャンプ
- Zed Extension Registry への公開（公式 extension は言語仕様安定後）
- Tree-sitter ハイライトグラマー（別プラン）

---

## 前提知識（コードを読んでいない実行者向け）

### エラー型の構造

すべてのフェーズのエラーは `span: Span` と `message: String` を持つ:

```rust
// src/span.rs
pub struct Span { pub offset: usize, pub len: usize }

// src/sema/resolve.rs
pub struct ResolveError { pub span: Span, pub message: String }

// src/sema/typeck.rs
pub struct TypeError { pub span: Span, pub message: String }

// src/sema/ownership/mod.rs
pub struct OwnershipError { pub span: Span, pub message: String, pub secondary: Option<(Span, String)> }

// src/lexer.rs
pub struct LexError { pub offset: usize, pub message: String }

// src/parser/error.rs
pub struct ParseErr { pub span: Span, pub message: String }
```

### PRELUDE のオフセット問題

`src/lib.rs` の `analyze()` は **プレリュード (`std/prelude.iris`) を先頭に自動前置**してから
解析する。つまりすべてのエラーの `span.offset` は `PRELUDE.len() + 1`（改行分）だけずれている。

LSP に渡す前にこのオフセットを引き、プレリュード範囲内のエラーは無視する必要がある。

```rust
let prelude_len = PRELUDE.len() + 1; // "\n" の分
// エラーのオフセット補正:
if span.offset < prelude_len { skip } else { span.offset - prelude_len }
```

### パイプライン構成（`src/lib.rs`）

```
src → PRELUDE前置 → lex → parse → resolve → typeck → check_ownership → codegen
```

現在の `analyze()` は private。LSP 向けに構造化エラーを返す新関数を追加する。

---

## Step 1: `src/lib.rs` に `pub fn check_diagnostics` を追加

**ファイル:** `src/lib.rs`

既存の `analyze()` と同様のロジックだが、`miette::Report` ではなく構造化エラーを返す。

追加する型と関数:

```rust
/// LSP 向けの診断エントリ。オフセットはユーザーソース先頭からのバイト数（プレリュード除く）。
pub struct LspDiagnostic {
    pub offset: usize,
    pub len: usize,
    pub message: String,
}

/// ソースを全フェーズで解析し、LSP 向けの診断リストを返す。
///
/// - エラーがなければ空ベクタ。
/// - プレリュード内のエラーは除外する（std のバグはユーザーに見せない）。
/// - `name` は診断に表示するファイル名（モジュール解決の基準ディレクトリにも使う）。
pub fn check_diagnostics(name: &str, src: &str) -> Vec<LspDiagnostic> {
    let prelude_len = PRELUDE.len() + 1; // PRELUDE + "\n"
    let combined = format!("{PRELUDE}\n{src}");
    let combined_src = combined.as_str();

    let mut diags: Vec<LspDiagnostic> = Vec::new();

    // --- 字句解析 ---
    let tokens = match lexer::lex(combined_src) {
        Ok(t) => t,
        Err(e) => {
            if e.offset >= prelude_len {
                diags.push(LspDiagnostic {
                    offset: e.offset - prelude_len,
                    len: 1,
                    message: e.message,
                });
            }
            return diags;
        }
    };

    // --- 構文解析 ---
    let mut program = match parser::parse(&tokens) {
        Ok(p) => p,
        Err(e) => {
            if e.span.offset >= prelude_len {
                diags.push(LspDiagnostic {
                    offset: e.span.offset - prelude_len,
                    len: e.span.len,
                    message: e.message,
                });
            }
            return diags;
        }
    };

    // --- モジュールロード ---
    let base_dir = Path::new(name).parent();
    let use_decls: Vec<UseDecl> = program
        .items
        .iter()
        .filter_map(|i| {
            if let Item::Use(u) = i {
                if u.path == ["std", "prelude"] { return None; }
                Some(u.clone())
            } else {
                None
            }
        })
        .collect();

    let (prepend_items, module_namespaces, _mod_errors) = if use_decls.is_empty() {
        (Vec::new(), HashMap::new(), Vec::new())
    } else {
        let mut loader = module::ModuleLoader::new(base_dir, combined.len());
        loader.process_use_decls(&use_decls)
    };
    // mod_errors は LSP では無視（ファイル未保存 / 解決失敗は silent）

    if !prepend_items.is_empty() {
        let mut new_items = prepend_items;
        new_items.extend(program.items);
        program.items = new_items;
    }

    // --- 名前解決 ---
    let resolution = match sema::resolve(&program, module_namespaces) {
        Ok(r) => r,
        Err(errors) => {
            for e in errors {
                if e.span.offset >= prelude_len {
                    diags.push(LspDiagnostic {
                        offset: e.span.offset - prelude_len,
                        len: e.span.len,
                        message: e.message,
                    });
                }
            }
            return diags;
        }
    };

    // --- 型検査 ---
    let type_info = match sema::check(&program, &resolution) {
        Ok(ti) => ti,
        Err(errors) => {
            for e in errors {
                if e.span.offset >= prelude_len {
                    diags.push(LspDiagnostic {
                        offset: e.span.offset - prelude_len,
                        len: e.span.len,
                        message: e.message,
                    });
                }
            }
            return diags;
        }
    };

    // --- 所有権検査 ---
    if let Err(errors) = sema::check_ownership(&program, &resolution, &type_info) {
        for e in errors {
            if e.span.offset >= prelude_len {
                diags.push(LspDiagnostic {
                    offset: e.span.offset - prelude_len,
                    len: e.span.len,
                    message: e.message,
                });
            }
            // secondary span はメモ程度なので LSP では省略
        }
    }

    diags
}
```

**実装の場所:** `src/lib.rs` の末尾、既存 `compile_ir` の後に追加。

**追加が必要な import:** `LspDiagnostic` 型は `lib.rs` 内で定義するため import 不要。
既存の `use` 行（`HashMap`, `Path`, `ast::{Item, UseDecl}` など）はすでに存在する。

**検証コマンド:**
```sh
cargo build 2>&1
# → エラーゼロ
cargo test 2>&1 | tail -5
# → test result: ok. N passed
```

---

## Step 2: Cargo ワークスペース化

現在 `Cargo.toml` は単一パッケージ。`iris-lsp` を追加するためワークスペースに変換する。

**ファイル:** `Cargo.toml`（上書き）

```toml
[workspace]
members = [".", "iris-lsp"]
resolver = "2"

[package]
name = "iris-lang"
version = "0.1.0"
edition = "2024"

[dependencies]
miette = {version = "7.6.0", features = ["fancy"]}
nom = "8.0.0"
nom_locate = "5.0.0"
thiserror = "2.0"
```

**検証コマンド:**
```sh
cargo build 2>&1
# → ワークスペース全体がビルドできること
```

---

## Step 3: `iris-lsp/Cargo.toml` を作る

**ファイル:** `iris-lsp/Cargo.toml`（新規作成）

```toml
[package]
name = "iris-lsp"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "iris-lsp"
path = "src/main.rs"

[dependencies]
iris-lang = { path = ".." }
lsp-server = "0.7"
lsp-types = "0.95"
serde_json = "1"
```

---

## Step 4: `iris-lsp/src/main.rs` を作る

LSP サーバーのメイン実装。`lsp-server` クレートを使った同期 JSON-RPC ループ。

**対応する LSP メソッド（最小限）:**
- `initialize` → capabilities を返す
- `initialized` → 無視
- `shutdown` / `exit` → 終了
- `textDocument/didOpen` → 診断を計算して publish
- `textDocument/didChange` → 診断を計算して publish

**ファイル:** `iris-lsp/src/main.rs`（新規作成）

```rust
//! iris-lang LSP サーバー。診断（errors）のみを提供する最小実装。

use iris_lang::{LspDiagnostic, check_diagnostics};
use lsp_server::{Connection, ExtractError, Message, Notification, Request, RequestId, Response};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::request::{Initialize, Shutdown};
use lsp_types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::FULL,
        )),
        ..Default::default()
    };

    let initialize_params: InitializeParams = {
        let (id, params) = connection.initialize_start()?;
        let result = InitializeResult {
            capabilities: server_capabilities,
            server_info: Some(ServerInfo {
                name: "iris-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        };
        connection.initialize_finish(id, serde_json::to_value(result)?)?;
        serde_json::from_value(params)?
    };
    let _ = initialize_params;

    // テキストキャッシュ: uri → (path, content)
    let mut docs: HashMap<Url, String> = HashMap::new();

    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    break;
                }
            }
            Message::Notification(notif) => {
                handle_notification(notif, &mut docs, &connection)?;
            }
            Message::Response(_) => {}
        }
    }

    io_threads.join()?;
    Ok(())
}

fn handle_notification(
    notif: Notification,
    docs: &mut HashMap<Url, String>,
    conn: &Connection,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    if notif.method == DidOpenTextDocument::METHOD {
        let params: DidOpenTextDocumentParams = serde_json::from_value(notif.params)?;
        let uri = params.text_document.uri;
        let text = params.text_document.text;
        publish_diagnostics(&uri, &text, conn)?;
        docs.insert(uri, text);
    } else if notif.method == DidChangeTextDocument::METHOD {
        let params: DidChangeTextDocumentParams = serde_json::from_value(notif.params)?;
        let uri = params.text_document.uri;
        // FULL sync: 最後の change のみ使う
        if let Some(change) = params.content_changes.into_iter().last() {
            publish_diagnostics(&uri, &change.text, conn)?;
            docs.insert(uri, change.text);
        }
    }
    Ok(())
}

fn publish_diagnostics(
    uri: &Url,
    text: &str,
    conn: &Connection,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let path = uri.path();
    let raw_diags = check_diagnostics(path, text);

    let diagnostics = raw_diags
        .into_iter()
        .map(|d| {
            let (start, end) = byte_offset_to_range(text, d.offset, d.len);
            Diagnostic {
                range: Range { start, end },
                severity: Some(DiagnosticSeverity::ERROR),
                message: d.message,
                source: Some("iris".to_string()),
                ..Default::default()
            }
        })
        .collect::<Vec<_>>();

    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version: None,
    };

    conn.sender.send(Message::Notification(Notification {
        method: "textDocument/publishDiagnostics".to_string(),
        params: serde_json::to_value(params)?,
    }))?;

    Ok(())
}

/// バイトオフセット + 長さ → LSP の (start, end) Position（行・列は UTF-16 単位）。
///
/// ponytail: UTF-16 列カウントは正確だが、純 ASCII ソースでは無意味なコスト。
/// iris ソース（英数字・日本語コメントは行末のみ）では実害なし。
fn byte_offset_to_range(text: &str, offset: usize, len: usize) -> (Position, Position) {
    let start = byte_to_position(text, offset);
    let end = byte_to_position(text, (offset + len).min(text.len()));
    (start, end)
}

fn byte_to_position(text: &str, byte_offset: usize) -> Position {
    let byte_offset = byte_offset.min(text.len());
    let before = &text[..byte_offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() as u32;
    let last_newline = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let col_utf16 = before[last_newline..]
        .chars()
        .map(|c| c.len_utf16() as u32)
        .sum::<u32>();
    Position { line, character: col_utf16 }
}
```

**検証コマンド:**
```sh
cargo build -p iris-lsp 2>&1
# → エラーゼロ
```

---

## Step 5: バイナリをインストールする

```sh
cargo install --path iris-lsp
# → ~/.cargo/bin/iris-lsp にインストールされる
iris-lsp --version  # 起動確認（起動後 stdin を待つので Ctrl-C で止める）
```

---

## Step 6: Zed の `settings.json` を編集する

Zed は公式 extension がなくても `settings.json` で LSP を直接設定できる。

**ファイル:** `~/.config/zed/settings.json`（既存設定に **追記** する。上書きしないこと）

```jsonc
{
  // ... 既存の設定 ...

  // .iris ファイルを "Iris" 言語として認識する
  "file_types": {
    "Iris": ["iris"]
  },

  // "Iris" 言語に iris-lsp を紐付ける
  "languages": {
    "Iris": {
      "language_servers": ["iris-lsp"],
      "tab_size": 4
    }
  },

  // iris-lsp バイナリのパスを指定する
  "lsp": {
    "iris-lsp": {
      "binary": {
        "path": "/home/mikan/.cargo/bin/iris-lsp",
        "args": []
      }
    }
  }
}
```

**注意:** `path` はフルパスで書く（`~` は展開されない環境がある）。
`which iris-lsp` で実際のパスを確認してから設定すること。

---

## Step 7: 動作確認

1. Zed を再起動する（または `Cmd/Ctrl+Shift+P` → "reload settings"）
2. 任意の `.iris` ファイルを Zed で開く
3. 故意にエラーを書く:
   ```
   fn main(): i32 {
       let x: i32 = "hello"
       return 0
   }
   ```
4. エラー波線が `"hello"` に表示されること、ホバーでメッセージが読めることを確認する

**STOP 条件:** 波線が出ない場合:
- `~/.config/zed/settings.json` が JSON として正しいか確認（末尾カンマ・コメントの扱いに注意。Zed は JSONC を読むがパーサが厳しい場合がある）
- Zed の言語サーバーログを確認: `Cmd/Ctrl+Shift+P` → "open language server logs"
- `iris-lsp` バイナリが実行可能か確認: `ls -la ~/.cargo/bin/iris-lsp`

---

## 変更ファイル一覧

| ファイル | 操作 |
|---|---|
| `Cargo.toml` | workspace 宣言を追加 |
| `src/lib.rs` | `LspDiagnostic` 型と `check_diagnostics()` を追加 |
| `iris-lsp/Cargo.toml` | 新規作成 |
| `iris-lsp/src/main.rs` | 新規作成 |
| `~/.config/zed/settings.json` | LSP/言語設定を追記（リポジトリ外） |

## スコープ外（触らないこと）

- `src/diagnostics.rs` — miette 診断は既存コードのまま
- `src/main.rs` — CLI は変更しない
- `tests/` — 既存テストは変更しない（追加は可）
- `.cargo/config.toml` — 作らない

## Done 条件

```sh
cargo test 2>&1 | tail -3
# → test result: ok. N passed; 0 failed
cargo build -p iris-lsp 2>&1
# → Finished ... iris-lsp
iris-lsp --help 2>&1 || true
# → usage または起動待ち（exit 0 以外でも可）
```

さらに Zed で `.iris` ファイルを開き、型エラーが波線として表示されること。

## 保守ノート

- `PRELUDE` の内容が変わるとオフセット補正値 (`PRELUDE.len() + 1`) が自動追従するため
  ハードコードしなくてよい（`const` 参照のまま）。
- LSP 仕様変更で `lsp-types` / `lsp-server` のメジャーバージョンが上がった場合は
  `Cargo.toml` の依存バージョンを上げる（breaking changes あり）。
- 補完・ホバーを追加する場合は `ServerCapabilities` に対応する能力を追加し、
  メインループの `Message::Request` ハンドラで処理する。

## エスケープハッチ

- **`lsp-server` が `cargo add` でインストールできない**: crates.io に存在する（`0.7.x`）ので
  `Cargo.toml` に手で追記して `cargo build` すること。
- **`lsp-types` バージョン不一致**: `lsp-server 0.7` は `lsp-types 0.95` と互換。
  `cargo tree -p iris-lsp` でバージョンを確認し、`lsp-types` の `features` が不足なら追加する。
- **Zed が settings.json を拒否する**: JSONC コメント（`//`）を削除してから試す。
- **iris-lsp がパニックする**: `iris-lang` の `check_diagnostics` が panic するケースは
  codegen 未対応の AST ノード（`unreachable!` マクロ）に到達した場合。LSP 向けには
  codegen を呼ばないためほぼ起きないはずだが、万一起きたら `check_diagnostics` の
  各フェーズを `std::panic::catch_unwind` で囲んで空リストを返すフォールバックを追加する。
