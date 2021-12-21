use std::future::Future;

use crate::{
    command::{Code, CommandType, Side},
    Command, IrcConnection, Result, Shutdown,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::*,
};

#[derive(Debug, Clone)]
enum ServerClientBroadcast {
    PrivMessage { channel: String, command: Command },
    Join { command: Command },
}

/// Starts the IRC Server and waits for it to complete.
/// `shutdown` allows you to pass in a future that will allow early termination with clean shutdowns for each connection
pub async fn run(listener: TcpListener, shutdown: impl Future) {
    let (notify_shutdown, _) = broadcast::channel(1);
    let (shutdown_complete_tx, shutdown_complete_rx) = mpsc::channel(1);
    let (server_tx, server_rx) = mpsc::channel(20);
    let (client_tx, _) = broadcast::channel(20);

    // Initialize the listener state
    let mut server = Server {
        listener,
        client_tx,
        server_tx,
        server_rx,
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
    client_tx: broadcast::Sender<ServerClientBroadcast>,
    // Server messages
    server_tx: mpsc::Sender<Command>,
    server_rx: mpsc::Receiver<Command>,
    // Graceful shutdown
    notify_shutdown: broadcast::Sender<()>,
    shutdown_complete_rx: mpsc::Receiver<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
}

impl Server {
    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                socket = self.listener.accept() => {
                    self.accept_client(socket?.0).await?;
                }
                broadcast = self.server_rx.recv() => {
                    if let Some(x) = broadcast {
                        self.send_broadcast(x).await?;
                    } else {
                        // Something has gone critically wrong to get to this point
                        return Err(Box::new(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "server_tx broke")));
                    }
                }
            }
        }
    }

    async fn accept_client(&mut self, socket: TcpStream) -> Result<()> {
        let client_ip_for_logging = socket.peer_addr().unwrap().ip();

        let mut client_connection = ClientConnection {
            connection: IrcConnection::new(socket),
            server_tx: self.server_tx.clone(),
            client_rx: self.client_tx.subscribe(),
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

        Ok(())
    }

    async fn send_broadcast(&mut self, broadcast: Command) -> Result<()> {
        match broadcast.command {
            CommandType::PRIVMSG => {
                self.client_tx.send(ServerClientBroadcast::PrivMessage {
                    channel: broadcast.parameters[0].clone(),
                    command: broadcast,
                })?;
            }
            CommandType::JOIN => {
                self.client_tx
                    .send(ServerClientBroadcast::Join { command: broadcast })?;
            }
            _ => {}
        }

        // self.client_tx.send(broadcast)?;
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct ClientInfo {
    pub nickname: String,
    pub username: String,
    pub realname: String,
    pub channels: Vec<String>,
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
    // Sending messages upstream
    server_tx: mpsc::Sender<Command>,
    client_rx: broadcast::Receiver<ServerClientBroadcast>,
    // Graceful shutdown
    shutdown: Shutdown,
    _shutdown_complete: mpsc::Sender<()>,
}

impl ClientConnection {
    async fn run(&mut self) -> Result<()> {
        while !self.shutdown.is_shutdown() {
            let maybe_command = tokio::select! {
                res = self.connection.read_line() => {
                    let res = res?;
                    // Indicates client hangup
                    if res.is_none() {
                        self.quit_client().await?;
                        return Ok(());
                    }
                    Some(Command::parse(res.unwrap(), Side::Client)?)
                },
                res = self.client_rx.recv() => {
                    println!("RECEIVED CLIENT_RX!!! {:?}", res);

                    let command = res?;
                    match command {
                        ServerClientBroadcast::PrivMessage { channel, command } => {
                            let source = command.source.as_ref().unwrap();
                            if self.info.channels.contains(&channel) && source[1..] != self.info.username {
                                Some(command)
                            } else {
                                None
                            }
                        }
                        ServerClientBroadcast::Join { command } => {
                            Some(command)
                        }
                    }
                },
                _ = self.shutdown.recv() => {
                    self.quit_client().await?;
                    return Ok(());
                }
            };

            let mut command = match maybe_command {
                Some(command) => command,
                None => {
                    continue;
                }
            };

            // let mut command = Command::parse(frame, side)?;
            println!("Command: {:?}", command);

            match command.apply(self).await {
                Ok(Code::Fine) => {}
                Ok(Code::Broadcast) => {
                    // If we're rebroadcasting, we have to set the source to our username.
                    command.source = Some(format!(":{}", self.info.username));
                    command.side = Side::Server;
                    self.server_tx.send(command).await?;
                }
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
