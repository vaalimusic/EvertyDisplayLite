use anyhow::Result;
use multitor_ipc::{ArrangeMode, IpcRequest, IpcResponse, IPC_PIPE_NAME};
use tokio::io::{split, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc;
use tracing::{info, warn};

pub enum ServiceCommand {
    GetTopology(tokio::sync::oneshot::Sender<multitor_ipc::TopologyConfig>),
    SetActiveMonitor {
        id: u32,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    UpdateTopology {
        topology: multitor_ipc::TopologyConfig,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    SetPauseSwitching(bool),
    SetViewportEnabled(bool),
    AddMonitor {
        name: String,
        width: u32,
        height: u32,
        refresh_rate: u32,
        reply: tokio::sync::oneshot::Sender<Result<u32, String>>,
    },
    ConfirmMonitor {
        id: u32,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    RemoveMonitor {
        id: u32,
        reply: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    MoveMonitorLeft(u32),
    MoveMonitorRight(u32),
    SetNeighbors {
        monitor_id: u32,
        left: Option<u32>,
        right: Option<u32>,
        top: Option<u32>,
        bottom: Option<u32>,
    },
    AutoArrange(ArrangeMode),
}

pub struct IpcServer {
    cmd_sender: mpsc::Sender<ServiceCommand>,
}

impl IpcServer {
    pub fn new(cmd_sender: mpsc::Sender<ServiceCommand>) -> Self {
        Self { cmd_sender }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting IPC Named Pipe server on {}", IPC_PIPE_NAME);

        let mut server = ServerOptions::new()
            .first_pipe_instance(true)
            .create(IPC_PIPE_NAME)?;

        loop {
            server.connect().await?;
            let connected_server = server;
            server = ServerOptions::new().create(IPC_PIPE_NAME)?;

            let cmd_sender = self.cmd_sender.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_client(connected_server, cmd_sender).await {
                    warn!("IPC client connection closed with: {:?}", e);
                }
            });
        }
    }
}

async fn handle_client(
    stream: NamedPipeServer,
    cmd_sender: mpsc::Sender<ServiceCommand>,
) -> Result<()> {
    let (reader, mut writer) = split(stream);
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }

        let request: IpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = IpcResponse::Error(format!("Invalid request JSON: {}", e));
                let mut out = serde_json::to_string(&err_resp)?;
                out.push('\n');
                writer.write_all(out.as_bytes()).await?;
                continue;
            }
        };

        let response = match request {
            IpcRequest::GetProductCapabilities => {
                IpcResponse::ProductCapabilities(everty_product_policy::ACTIVE_CAPABILITIES)
            }
            IpcRequest::Ping => IpcResponse::Pong,
            IpcRequest::GetTopology => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                let _ = cmd_sender.send(ServiceCommand::GetTopology(tx)).await;
                if let Ok(top) = rx.await {
                    IpcResponse::Topology(top)
                } else {
                    IpcResponse::Error("Failed to retrieve topology".to_string())
                }
            }
            IpcRequest::SetActiveMonitor(id) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if cmd_sender
                    .send(ServiceCommand::SetActiveMonitor { id, reply: tx })
                    .await
                    .is_err()
                {
                    IpcResponse::Error("Service is shutting down".to_string())
                } else {
                    match rx.await {
                        Ok(Ok(())) => IpcResponse::Success,
                        Ok(Err(message)) => IpcResponse::Error(message),
                        Err(_) => IpcResponse::Error("Monitor switch was interrupted".to_string()),
                    }
                }
            }
            IpcRequest::UpdateTopology(top) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if cmd_sender
                    .send(ServiceCommand::UpdateTopology {
                        topology: top,
                        reply: tx,
                    })
                    .await
                    .is_err()
                {
                    IpcResponse::Error("Service is shutting down".to_string())
                } else {
                    match rx.await {
                        Ok(Ok(())) => IpcResponse::Success,
                        Ok(Err(message)) => IpcResponse::Error(message),
                        Err(_) => {
                            IpcResponse::Error("Display arrangement was interrupted".to_string())
                        }
                    }
                }
            }
            IpcRequest::SetPauseSwitching(pause) => {
                let _ = cmd_sender
                    .send(ServiceCommand::SetPauseSwitching(pause))
                    .await;
                IpcResponse::Success
            }
            IpcRequest::SetViewportEnabled(enabled) => {
                let _ = cmd_sender
                    .send(ServiceCommand::SetViewportEnabled(enabled))
                    .await;
                IpcResponse::Success
            }
            IpcRequest::AddMonitor {
                name,
                width,
                height,
                refresh_rate,
            } => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if cmd_sender
                    .send(ServiceCommand::AddMonitor {
                        name,
                        width,
                        height,
                        refresh_rate,
                        reply: tx,
                    })
                    .await
                    .is_err()
                {
                    IpcResponse::Error("Service is shutting down".to_string())
                } else {
                    match rx.await {
                        Ok(Ok(new_id)) => IpcResponse::MonitorAdded(new_id),
                        Ok(Err(message)) => IpcResponse::Error(message),
                        Err(_) => {
                            IpcResponse::Error("Monitor creation was interrupted".to_string())
                        }
                    }
                }
            }
            IpcRequest::RemoveMonitor(id) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if cmd_sender
                    .send(ServiceCommand::RemoveMonitor { id, reply: tx })
                    .await
                    .is_err()
                {
                    IpcResponse::Error("Service is shutting down".to_string())
                } else {
                    match rx.await {
                        Ok(Ok(())) => IpcResponse::MonitorRemoved(id),
                        Ok(Err(message)) => IpcResponse::Error(message),
                        Err(_) => IpcResponse::Error("Monitor removal was interrupted".to_string()),
                    }
                }
            }
            IpcRequest::ConfirmMonitor(id) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                if cmd_sender
                    .send(ServiceCommand::ConfirmMonitor { id, reply: tx })
                    .await
                    .is_err()
                {
                    IpcResponse::Error("Service is shutting down".to_string())
                } else {
                    match rx.await {
                        Ok(Ok(())) => IpcResponse::Success,
                        Ok(Err(message)) => IpcResponse::Error(message),
                        Err(_) => {
                            IpcResponse::Error("Monitor confirmation was interrupted".to_string())
                        }
                    }
                }
            }
            IpcRequest::MoveMonitorLeft(id) => {
                let _ = cmd_sender.send(ServiceCommand::MoveMonitorLeft(id)).await;
                IpcResponse::Success
            }
            IpcRequest::MoveMonitorRight(id) => {
                let _ = cmd_sender.send(ServiceCommand::MoveMonitorRight(id)).await;
                IpcResponse::Success
            }
            IpcRequest::SetNeighbors {
                monitor_id,
                left,
                right,
                top,
                bottom,
            } => {
                let _ = cmd_sender
                    .send(ServiceCommand::SetNeighbors {
                        monitor_id,
                        left,
                        right,
                        top,
                        bottom,
                    })
                    .await;
                IpcResponse::Success
            }
            IpcRequest::AutoArrange(mode) => {
                if mode == ArrangeMode::Grid2x2
                    && !everty_product_policy::ACTIVE_CAPABILITIES.multi_display_layouts
                {
                    IpcResponse::Error(
                        "The Lite edition does not support multi-display grid layouts".to_string(),
                    )
                } else {
                    let _ = cmd_sender.send(ServiceCommand::AutoArrange(mode)).await;
                    IpcResponse::Success
                }
            }
            IpcRequest::GetDisplays => {
                let displays = multitor_driver_manager::enumerate_displays().unwrap_or_default();
                IpcResponse::Displays(displays)
            }
        };

        let mut out = serde_json::to_string(&response)?;
        out.push('\n');
        writer.write_all(out.as_bytes()).await?;
        writer.flush().await?;
    }

    Ok(())
}
