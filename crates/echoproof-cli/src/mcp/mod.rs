//! Model Context Protocol server: newline-delimited JSON-RPC 2.0 over stdio.
//! stdout carries protocol messages only.

mod args;
mod schema;
mod tools;

use anyhow::Result;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

use tools::Server;

type RpcError = (i64, String);

pub async fn serve(config_path: PathBuf) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut stdout = tokio::io::stdout();
        while let Some(message) = rx.recv().await {
            let mut line = serde_json::to_vec(&message)?;
            line.push(b'\n');
            stdout.write_all(&line).await?;
            stdout.flush().await?;
        }
        anyhow::Ok(())
    });

    let server = Arc::new(Server::new(config_path)?);
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    eprintln!("echoproof mcp: ready on stdio");
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                let _ = tx.send(error_response(
                    Value::Null,
                    (-32700, format!("parse error: {error}")),
                ));
                continue;
            }
        };
        // One task per request, so a long replay does not block pings.
        let (server, tx) = (server.clone(), tx.clone());
        tokio::spawn(async move {
            if let Some(response) = handle(&server, message).await {
                let _ = tx.send(response);
            }
        });
    }
    drop(tx);
    writer.await??;
    Ok(())
}

fn error_response(id: Value, (code, message): RpcError) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

/// Notifications (no id) and responses (no method) get no reply.
async fn handle(server: &Server, message: Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method")?.as_str().unwrap_or_default();
    let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => Ok(schema::initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": schema::tools() })),
        "tools/call" => call_tool(server, &params).await,
        _ => Err((-32601, format!("method not found: {method}"))),
    };
    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => error_response(id, error),
    })
}

/// Tool failures are results with isError, so the agent can read and react.
async fn call_tool(server: &Server, params: &Value) -> Result<Value, RpcError> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let outcome = match name {
        "echoproof_check" => server.check(arguments).await,
        "echoproof_diff" => server.diff(arguments).await,
        "echoproof_exchange" => server.exchange(arguments),
        _ => return Err((-32602, format!("unknown tool: {name}"))),
    };
    Ok(match outcome {
        Ok((text, structured)) => json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": structured,
            "isError": false,
        }),
        Err(error) => json!({
            "content": [{ "type": "text", "text": format!("{error:#}") }],
            "isError": true,
        }),
    })
}
