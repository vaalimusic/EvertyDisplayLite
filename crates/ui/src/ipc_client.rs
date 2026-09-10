use anyhow::{bail, Context, Result};
use multitor_ipc::{IpcRequest, IpcResponse, IPC_PIPE_NAME};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::ClientOptions;

pub async fn send_ipc_request(req: &IpcRequest) -> Result<IpcResponse> {
    let timeout_seconds = if matches!(
        req,
        IpcRequest::AddMonitor { .. } | IpcRequest::RemoveMonitor(_)
    ) {
        30
    } else {
        3
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        send_ipc_request_inner(req),
    )
    .await
    .with_context(|| format!("Служба EvertyDisplay не ответила за {timeout_seconds} секунд"))?
}

async fn send_ipc_request_inner(req: &IpcRequest) -> Result<IpcResponse> {
    let client = ClientOptions::new().open(IPC_PIPE_NAME)?;
    let (reader, mut writer) = tokio::io::split(client);
    let mut lines = BufReader::new(reader).lines();

    let mut json = serde_json::to_string(req)?;
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;

    if let Some(resp_line) = lines.next_line().await? {
        let resp: IpcResponse = serde_json::from_str(&resp_line)?;
        Ok(resp)
    } else {
        bail!("Pipe closed without response");
    }
}
