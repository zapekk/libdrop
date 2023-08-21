use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use drop_core::Status;
use futures::SinkExt;
use slog::{debug, error, info, warn};
use tokio::{
    sync::mpsc::Sender,
    task::{AbortHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

use super::{
    handler::{self, MsgToSend},
    WebSocket,
};
use crate::{
    manager::FileTerminalState, protocol::v5 as prot, service::State, tasks::AliveGuard,
    transfer::Transfer, ws::events::FileEventTx, FileId, OutgoingTransfer,
};

pub struct HandlerInit<'a> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    alive: &'a AliveGuard,
}

pub struct HandlerLoop<'a> {
    state: &'a Arc<State>,
    logger: &'a slog::Logger,
    alive: &'a AliveGuard,
    upload_tx: Sender<MsgToSend>,
    tasks: HashMap<FileId, FileTask>,
    done: HashSet<FileId>,
    xfer: Arc<OutgoingTransfer>,
}

struct FileTask {
    job: AbortHandle,
    events: Arc<FileEventTx<OutgoingTransfer>>,
}

struct Uploader {
    sink: Sender<MsgToSend>,
    file_id: FileId,
    offset: u64,
}

impl<'a> HandlerInit<'a> {
    pub(crate) fn new(
        state: &'a Arc<State>,
        logger: &'a slog::Logger,
        alive: &'a AliveGuard,
    ) -> Self {
        Self {
            state,
            logger,
            alive,
        }
    }
}

#[async_trait::async_trait]
impl<'a> handler::HandlerInit for HandlerInit<'a> {
    type Pinger = tokio::time::Interval;
    type Loop = HandlerLoop<'a>;

    async fn start(
        &mut self,
        socket: &mut WebSocket,
        xfer: &OutgoingTransfer,
    ) -> crate::Result<()> {
        let req = prot::TransferRequest::from(xfer);
        socket.send(Message::from(&req)).await?;
        Ok(())
    }

    fn upgrade(self, upload_tx: Sender<MsgToSend>, xfer: Arc<OutgoingTransfer>) -> Self::Loop {
        let Self {
            state,
            logger,
            alive,
        } = self;

        HandlerLoop {
            state,
            alive,
            logger,
            upload_tx,
            xfer,
            tasks: HashMap::new(),
            done: HashSet::new(),
        }
    }

    fn pinger(&mut self) -> Self::Pinger {
        tokio::time::interval(self.state.config.ping_interval())
    }
}

impl HandlerLoop<'_> {
    async fn on_cancel(&mut self, file_id: FileId, by_peer: bool) {
        if let Some(task) = self.tasks.remove(&file_id) {
            if !task.job.is_finished() {
                task.job.abort();
                task.events.cancelled(by_peer).await;
            }
        }
    }

    async fn on_reject(&mut self, file_id: FileId) {
        info!(self.logger, "on reject file {file_id}");

        match self
            .state
            .transfer_manager
            .outgoing_terminal_recv(self.xfer.id(), &file_id, FileTerminalState::Rejected)
            .await
        {
            Err(err) => {
                error!(self.logger, "Failed to handler file rejection: {err}");
            }
            Ok(Some(res)) => res.events.rejected(true).await,
            Ok(None) => (),
        }

        self.stop_task(&file_id, Status::FileRejected).await;
    }

    async fn stop_task(&mut self, file_id: &FileId, status: Status) {
        if let Some(task) = self.tasks.remove(file_id) {
            if !task.job.is_finished() {
                debug!(
                    self.logger,
                    "Aborting upload job: {}:{file_id}",
                    self.xfer.id()
                );

                task.job.abort();
                task.events.stop_silent(status).await;
            }
        }
    }

    async fn on_progress(&self, file_id: FileId, transfered: u64) {
        if let Some(task) = self.tasks.get(&file_id) {
            task.events.progress(transfered).await;
        }
    }

    async fn on_done(&mut self, file_id: FileId) {
        if let Err(err) = self
            .state
            .transfer_manager
            .outgoing_terminal_recv(self.xfer.id(), &file_id, FileTerminalState::Completed)
            .await
        {
            warn!(self.logger, "Failed to accept file as done: {err}");
        }

        if let Some(task) = self.tasks.remove(&file_id) {
            task.events.success().await;
        } else if !self.done.contains(&file_id) {
            let event = crate::Event::FileUploadSuccess(self.xfer.clone(), file_id.clone());

            self.state
                .event_tx
                .send(event)
                .await
                .expect("Failed to emit event");
        }

        self.done.insert(file_id);
    }

    async fn on_checksum(&self, jobs: &mut JoinSet<()>, file_id: FileId, limit: u64) {
        let state = self.state.clone();
        let msg_tx = self.upload_tx.clone();
        let xfer = self.xfer.clone();
        let logger = self.logger.clone();
        let alive = self.alive.clone();

        let task = async move {
            let _guard = alive;

            let make_report = async {
                state
                    .transfer_manager
                    .outgoing_ensure_file_not_terminated(xfer.id(), &file_id)
                    .await?;

                let checksum = xfer.files()[&file_id].checksum(limit).await?;

                crate::Result::Ok(prot::ReportChsum {
                    file: file_id.clone(),
                    limit,
                    checksum,
                })
            };

            match make_report.await {
                Ok(report) => {
                    let _ = msg_tx
                        .send(MsgToSend::from(&prot::ClientMsg::ReportChsum(report)))
                        .await;
                }
                Err(err) => {
                    error!(logger, "Failed to report checksum: {:?}", err);

                    let msg = err.to_string();

                    match state
                        .transfer_manager
                        .outgoing_failure_post(xfer.id(), &file_id)
                        .await
                    {
                        Err(err) => {
                            warn!(logger, "Failed to post failure {err:?}");
                        }
                        Ok(res) => res.events.failed(err).await,
                    }

                    let msg = prot::Error {
                        file: Some(file_id.clone()),
                        msg,
                    };
                    let _ = msg_tx
                        .send(MsgToSend {
                            msg: Message::from(&prot::ClientMsg::Error(msg)),
                        })
                        .await;
                }
            }
        };

        jobs.spawn(task);
    }

    async fn on_start(
        &mut self,
        socket: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        file_id: FileId,
        offset: u64,
    ) -> anyhow::Result<()> {
        let start = async {
            self.state
                .transfer_manager
                .outgoing_ensure_file_not_terminated(self.xfer.id(), &file_id)
                .await?;

            let start = || {
                let uploader = Uploader {
                    sink: self.upload_tx.clone(),
                    file_id: file_id.clone(),
                    offset,
                };
                let state = self.state.clone();
                let alive = self.alive.clone();
                let logger = self.logger.clone();
                let xfer = self.xfer.clone();
                let file_id = file_id.clone();

                async move {
                    let (job, events) =
                        super::start_upload(jobs, state, alive, logger, uploader, xfer, file_id)
                            .await?;

                    anyhow::Ok(FileTask { job, events })
                }
            };

            match self.tasks.entry(file_id.clone()) {
                Entry::Occupied(o) => {
                    let task = o.into_mut();

                    if task.job.is_finished() {
                        *task = start().await?;
                    } else {
                        anyhow::bail!("Transfer already in progress");
                    }
                }
                Entry::Vacant(v) => {
                    v.insert(start().await?);
                }
            };

            self.done.remove(&file_id);
            anyhow::Ok(())
        };

        if let Err(err) = start.await {
            error!(self.logger, "Failed to start upload: {:?}", err);

            let msg = prot::Error {
                file: Some(file_id),
                msg: err.to_string(),
            };

            socket
                .send(Message::from(&prot::ClientMsg::Error(msg)))
                .await
                .context("Failed to report error")?;
        }

        Ok(())
    }

    async fn on_error(&mut self, file_id: Option<FileId>, msg: String) {
        error!(
            self.logger,
            "Server reported and error: file: {file_id:?}, message: {msg}",
        );

        if let Some(file_id) = file_id {
            match self
                .state
                .transfer_manager
                .outgoing_terminal_recv(self.xfer.id(), &file_id, FileTerminalState::Failed)
                .await
            {
                Err(err) => {
                    warn!(self.logger, "Failed to accept failure: {err}");
                }
                Ok(Some(res)) => {
                    res.events
                        .failed(crate::Error::BadTransferState(format!(
                            "Receiver reported an error: {msg}"
                        )))
                        .await;
                }
                Ok(None) => (),
            }

            self.stop_task(&file_id, Status::BadTransferState).await;
        }
    }
}

#[async_trait::async_trait]
impl handler::HandlerLoop for HandlerLoop<'_> {
    async fn issue_reject(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let msg = prot::ClientMsg::Reject(prot::Reject {
            file: file_id.clone(),
        });
        socket.send(Message::from(&msg)).await?;

        self.stop_task(&file_id, Status::FileRejected).await;

        Ok(())
    }

    async fn issue_faliure(
        &mut self,
        socket: &mut WebSocket,
        file_id: FileId,
    ) -> anyhow::Result<()> {
        let msg = prot::ClientMsg::Error(prot::Error {
            file: Some(file_id),
            msg: String::from("File failed elsewhere"),
        });
        socket.send(Message::from(&msg)).await?;

        Ok(())
    }

    async fn on_close(&mut self, by_peer: bool) {
        debug!(self.logger, "ClientHandler::on_close(by_peer: {})", by_peer);

        self.on_stop().await;

        if by_peer {
            self.state
                .event_tx
                .send(crate::Event::OutgoingTransferCanceled(
                    self.xfer.clone(),
                    by_peer,
                ))
                .await
                .expect("Could not send a transfer cancelled event, channel closed");
        }
    }

    async fn on_text_msg(
        &mut self,
        socket: &mut WebSocket,
        jobs: &mut JoinSet<()>,
        text: String,
    ) -> anyhow::Result<()> {
        let msg: prot::ServerMsg =
            serde_json::from_str(&text).context("Failed to deserialize server message")?;

        match msg {
            prot::ServerMsg::Progress(prot::Progress {
                file,
                bytes_transfered,
            }) => self.on_progress(file, bytes_transfered).await,
            prot::ServerMsg::Done(prot::Done {
                file,
                bytes_transfered: _,
            }) => self.on_done(file).await,
            prot::ServerMsg::Error(prot::Error { file, msg }) => self.on_error(file, msg).await,
            prot::ServerMsg::ReqChsum(prot::ReqChsum { file, limit }) => {
                self.on_checksum(jobs, file, limit).await
            }
            prot::ServerMsg::Start(prot::Start { file, offset }) => {
                self.on_start(socket, jobs, file, offset).await?
            }
            prot::ServerMsg::Cancel(prot::Cancel { file }) => self.on_cancel(file, true).await,
            prot::ServerMsg::Reject(prot::Reject { file }) => self.on_reject(file).await,
        }
        Ok(())
    }

    async fn on_stop(&mut self) {
        debug!(self.logger, "Waiting for background jobs to finish");

        let tasks = self.tasks.drain().map(|(_, task)| async move {
            task.events.stop_silent(Status::Canceled).await;
        });

        futures::future::join_all(tasks).await;
    }

    fn recv_timeout(&mut self, last_recv_elapsed: Duration) -> Option<Duration> {
        Some(
            self.state
                .config
                .transfer_idle_lifetime
                .saturating_sub(last_recv_elapsed),
        )
    }
}
impl Drop for HandlerLoop<'_> {
    fn drop(&mut self) {
        debug!(self.logger, "Stopping client handler");

        let jobs = std::mem::take(&mut self.tasks);
        tokio::spawn(async move {
            let tasks = jobs.into_values().map(|task| async move {
                task.events.pause().await;
            });

            futures::future::join_all(tasks).await;
        });
    }
}

#[async_trait::async_trait]
impl handler::Uploader for Uploader {
    async fn chunk(&mut self, chunk: &[u8]) -> Result<(), crate::Error> {
        let msg = prot::Chunk {
            file: self.file_id.clone(),
            data: chunk.to_vec(),
        };

        self.sink
            .send(MsgToSend {
                msg: Message::from(msg),
            })
            .await
            .map_err(|_| crate::Error::Canceled)?;

        Ok(())
    }

    async fn error(&mut self, msg: String) {
        let msg = prot::ClientMsg::Error(prot::Error {
            file: Some(self.file_id.clone()),
            msg,
        });

        let _ = self
            .sink
            .send(MsgToSend {
                msg: Message::from(&msg),
            })
            .await;
    }

    fn offset(&self) -> u64 {
        self.offset
    }
}
