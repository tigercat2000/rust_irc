use std::future::Future;

use crate::{command::Code, Command, IrcConnection, Result, Shutdown};
use tokio::{net::TcpListener, sync::*};

/// Starts the IRC Server and waits for it to complete.
/// `shutdown` allows you to pass in a future that will allow early termination with clean shutdowns for each connection
pub async fn run(listener: TcpListener, shutdown: impl Future) {
    let (notify_shutdown, _) = broadcast::channel(1);
    let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(1);

    // Initialize the listener state
    let mut server = Server {
        listener,
        notify_shutdown,
        shutdown_complete_tx,
        shutdown_complete_rx,
    };

    // select! runs both tasks at the same time
    tokio::select! {
        res = server.run() => {
            if let Err(err) = res {
                println!("Failed to accept: {}", err);
            }
        }
        _ = shutdown => {
            println!("Shutting down");
        }
    }

    let Server {
        mut shutdown_complete_rx,
        shutdown_complete_tx,
        notify_shutdown,
        ..
    } = server;

    drop(notify_shutdown);
    drop(shutdown_complete_tx);

    let _ = shutdown_complete_rx.recv().await;
}

#[derive(Debug)]
struct Server {
    listener: TcpListener,
    // Graceful shutdown
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_rx: mpsc::Receiver<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
}

impl Server {
    async fn run(&mut self) -> Result<()> {
        loop {
            let (socket, _) = self.listener.accept().await?;

            let client_ip_for_logging = socket.peer_addr().unwrap().ip();

            let mut client_connection = ClientConnection {
                connection: IrcConnection::new(socket),
                shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
                _shutdown_complete: self.shutdown_complete_tx.clone(),
                info: ClientInfo::default(),
            };

            tokio::spawn(async move {
                if let Err(e) = client_connection.run().await {
                    eprintln!("ERROR: {}", e);
                }
                println!("Client {} disconnected.", client_ip_for_logging);
            });
        }
    }
}

#[derive(Debug, Default)]
pub struct ClientInfo {
    pub nickname: String,
    pub username: String,
    pub realname: String,
}

impl ClientInfo {
    /// Converts our struct into the canonical form of the user identifier.
    pub fn to_canonical<S: AsRef<str>>(&self, server: S) -> String {
        format!("{}!{}@{}", self.nickname, self.username, server.as_ref())
    }
}

#[derive(Debug)]
pub struct ClientConnection {
    pub connection: IrcConnection,
    pub info: ClientInfo,
    // Graceful shutdown
    shutdown: Shutdown,
    _shutdown_complete: mpsc::Sender<()>,
}

impl ClientConnection {
    async fn run(&mut self) -> Result<()> {
        while !self.shutdown.is_shutdown() {
            let maybe_frame = tokio::select! {
                res = self.connection.read_line() => res?,
                _ = self.shutdown.recv() => {
                    self.quit_client().await?;
                    return Ok(());
                }
            };

            let frame = match maybe_frame {
                Some(frame) => frame,
                None => {
                    self.quit_client().await?;
                    return Ok(());
                }
            };

            let command = Command::parse(frame)?;
            println!("Command: {:?}", command);

            match command.apply(self).await {
                Ok(Code::Fine) => {}
                Ok(Code::Exit) => return Ok(()),
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    async fn quit_client(&mut self) -> Result<()> {
        self.connection
            .write_quit(&self.info, "Quit: Server shutting down.")
            .await?;
        self.connection.write_error("Server shutting down.").await?;
        Ok(())
    }
}
