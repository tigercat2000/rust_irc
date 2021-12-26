use std::future::Future;

use crate::{
    message_impl::Code,
    message_parse::{Command, Message, Side},
    IrcConnection, Result, Shutdown,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::*,
};

#[derive(Debug, Clone)]
enum ServerClientBroadcast {
    PrivMessage {
        channels: Vec<String>,
        message: Message,
    },
    Join {
        message: Message,
    },
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
    /// This is the TcpListener which new clients connect to, forming a TcpStream that is then tokio-spawned off
    listener: TcpListener,
    /// This is how we tell clients that we
    client_tx: broadcast::Sender<ServerClientBroadcast>,
    // Server messages
    /// We don't use this, but we need to hold it somewhere in memory and this struct is convenient
    server_tx: mpsc::Sender<Message>,
    /// This is what we actually use, clients send message on tx and we get it on rx
    server_rx: mpsc::Receiver<Message>,
    // Graceful shutdown
    /// This broadcasts a shutdown signal to all active connections
    notify_shutdown: broadcast::Sender<()>,
    /// Used to wait until client connections are finished closing- tokio channels close when all senders go out of scope.
    shutdown_complete_rx: mpsc::Receiver<()>,
    shutdown_complete_tx: mpsc::Sender<()>,
}

impl Server {
    /// This is the main loop for the Server, it listens eternally for new clients and simultaneously listens for
    /// old clients that want to talk to it about something
    async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                // New client
                socket = self.listener.accept() => {
                    self.accept_client(socket?.0).await?;
                }
                // Established client asking us for something
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

    /// This accepts a new TcpStream and establishes all the internal structs to control the connection before
    /// tokio-spawning it off to handle itself (we just talk to it with channels)
    async fn accept_client(&mut self, socket: TcpStream) -> Result<()> {
        let client_ip_for_logging = socket.peer_addr().unwrap().ip();

        let mut client_connection = ClientConnection {
            // Wrapper for the IRC protocol around the tcpstream
            connection: IrcConnection::new(socket),
            // It gets to ask us for stuff
            server_tx: self.server_tx.clone(),
            // And we get to ask it for stuff
            client_rx: self.client_tx.subscribe(),
            // Shutdown is the wrapper that helps the client know when it's time to die
            shutdown: Shutdown::new(self.notify_shutdown.subscribe()),
            // We also bind a shutdown_complete_tx to it's lifetime so that we can wait on shutdown_complete_rx
            // to finish before we exit the program
            _shutdown_complete: self.shutdown_complete_tx.clone(),
            // Internal information for the connection
            info: ClientInfo::default(),
        };

        // Client can handle itself now
        tokio::spawn(async move {
            if let Err(e) = client_connection.run().await {
                eprintln!("ERROR: {}", e);
            }
            println!("Client {} disconnected.", client_ip_for_logging);
        });

        Ok(())
    }

    /// This handles all messages that the client threads ask the server to do
    async fn send_broadcast(&mut self, broadcast: Message) -> Result<()> {
        match &broadcast.command {
            Command::PRIVMSG(targets, _) => {
                self.client_tx.send(ServerClientBroadcast::PrivMessage {
                    channels: targets.clone(),
                    message: broadcast,
                })?;
            }
            Command::JOIN(_, _) => {
                self.client_tx
                    .send(ServerClientBroadcast::Join { message: broadcast })?;
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
    /// Wrapper around a TcpStream that gives us easy functions for the IRC protocol
    pub connection: IrcConnection,
    /// Information about the connection that we need stored somewhere
    pub info: ClientInfo,
    /// We use this to ask the server to do stuff
    server_tx: mpsc::Sender<Message>,
    /// We receive on this to do stuff when the server asks us to
    client_rx: broadcast::Receiver<ServerClientBroadcast>,
    /// We run this helper and wait until it tells us to die
    shutdown: Shutdown,
    /// When we Drop this Drops and the server can tell we're dead
    _shutdown_complete: mpsc::Sender<()>,
}

impl ClientConnection {
    /// Main loop of the client handler
    async fn run(&mut self) -> Result<()> {
        // so we don't have to wait on select! between commands
        while !self.shutdown.is_shutdown() {
            // This is the main branching logic for the client
            // not all branches return commands
            let maybe_command = tokio::select! {
                // Our client sent us something, handle it
                res = self.connection.read_line() => {
                    let res = res?;
                    // Indicates client hangup
                    if res.is_none() {
                        self.quit_client().await?;
                        return Ok(());
                    }
                    let mut message: Message = res.unwrap().parse()?;
                    message.side = Side::Client;
                    Some(message)
                },
                // The server told us to do something, handle it
                res = self.client_rx.recv() => {
                    let command = res?;
                    match command {
                        ServerClientBroadcast::PrivMessage { channels, message } => {
                            if let Some(source) = &message.source {
                                if source != &self.info.username && self.info.channels.iter().any(|a| channels.contains(a)) {
                                    Some(message.clone())
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        ServerClientBroadcast::Join { message } => {
                            Some(message)
                        }
                    }
                },
                // The server told us it's dying time, handle it
                _ = self.shutdown.recv() => {
                    self.quit_client().await?;
                    return Ok(());
                }
            };

            // We share this between two select branches, the client
            // only ever acts on Messages (TODO)
            let mut command = match maybe_command {
                Some(command) => command,
                None => {
                    continue;
                }
            };

            // let mut command = Message::parse(frame, side)?;
            // println!("Message: {:?}", command);

            // Let the command do it's damage
            match command.apply(self).await {
                // It did something but we don't care
                Ok(Code::Fine) => {}
                // It did something and we need the server to care
                Ok(Code::Broadcast) => {
                    // If we're rebroadcasting, we have to set the source to our username.
                    command.source = Some(self.info.username.clone());
                    command.side = Side::Server;
                    self.server_tx.send(command).await?;
                }
                // It did something and we're dying now
                Ok(Code::Exit) => return Ok(()),
                // It did something really bad and we're dying extra hard now
                Err(e) => {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// This is a helper to clean ourselves up, we don't use Drop because we need async to interact with our async socket
    async fn quit_client(&mut self) -> Result<()> {
        self.connection
            .write_quit(&self.info, "Quit: Server shutting down.")
            .await?;
        self.connection.write_error("Server shutting down.").await?;
        Ok(())
    }
}
