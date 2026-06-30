//! iris-lang LSP サーバー。診断（errors）のみを提供する最小実装。

use iris_lang::check_diagnostics;
use lsp_server::{Connection, Message, Notification};
use lsp_types::notification::{DidChangeTextDocument, DidOpenTextDocument, Notification as _};
use lsp_types::*;
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

    // テキストキャッシュ: uri → content
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
