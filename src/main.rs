// use std::io::{prelude::*, BufReader};
// use std::net::{Shutdown, TcpListener, TcpStream};
use std::io::Result;
use std::net::SocketAddr;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch::{self, Receiver, Sender};

/// Holds information about a client for a given connection.
#[derive(Debug, Default)]
struct ClientInfo {
    pub nickname: String,
    pub username: String,
    pub realname: String,
}

impl ClientInfo {
    /// Converts our struct into the canonical form of the user identifier.
    fn to_canonical<S: AsRef<str>>(&self, server: S) -> String {
        format!("{}!{}@{}", self.nickname, self.username, server.as_ref())
    }
}

#[repr(usize)]
#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types)]
enum NumericReply {
    RPL_WELCOME = 1,
    RPL_YOURHOST = 2,
    RPL_CREATED = 3,
    RPL_MYINFO = 4,
    RPL_ISUPPORT = 5,
    RPL_MOTDSTART = 375,
    RPL_MOTD = 372,
    RPL_ENDOFMOTD = 376,
    ERR_UNKNOWN_COMMAND = 421,
}

impl ToString for NumericReply {
    fn to_string(&self) -> String {
        format!("{:0>3}", *self as usize)
    }
}

/// Wrapper around TcpStream that handles common write operations for IRC traffic.
#[allow(dead_code)]
struct IrcWriter {
    stream: OwnedWriteHalf,
    server_addr: SocketAddr,
    client_addr: SocketAddr,
}

impl IrcWriter {
    /// Make a new IrcWriter for `stream`.
    fn new(stream: OwnedWriteHalf, server_addr: SocketAddr, client_addr: SocketAddr) -> Self {
        Self {
            stream,
            server_addr,
            client_addr,
        }
    }

    /// Sends the numeric reply sequence for the MOTD.
    async fn motd(&mut self, client: &ClientInfo) -> Result<()> {
        self.numeric_reply(
            client,
            NumericReply::RPL_MOTDSTART,
            format!("- {} Message of the day - ", self.server_addr.ip()),
        )
        .await?;
        self.numeric_reply(client, NumericReply::RPL_MOTD, "- Hi from Rust-IRC!")
            .await?;
        self.numeric_reply(client, NumericReply::RPL_ENDOFMOTD, "End of /MOTD command")
            .await?;
        Ok(())
    }

    /// This is the 5 packet series required after a registration has finished.
    async fn registration_reply(&mut self, client: &ClientInfo) -> Result<()> {
        self.numeric_reply(
            client,
            NumericReply::RPL_WELCOME,
            format!(
                "Welcome to the Internet Relay Network {}",
                client.to_canonical(self.server_addr.ip().to_string())
            ),
        )
        .await?;
        self.numeric_reply(
            client,
            NumericReply::RPL_YOURHOST,
            format!(
                "Your host is {}, running version rust_irc-0.0.0",
                self.server_addr.ip()
            ),
        )
        .await?;
        self.numeric_reply(
            client,
            NumericReply::RPL_CREATED,
            "This server was created... probably 10 seconds ago who cares",
        )
        .await?;
        self.numeric_reply_notrailer(
            client,
            NumericReply::RPL_MYINFO,
            format!(
                "{} {} {} {}",
                self.server_addr.ip(),
                "rust_irc-0.0.0",
                " ",
                " "
            ),
        )
        .await?;
        self.numeric_reply_notrailer(
            client,
            NumericReply::RPL_ISUPPORT,
            "CASEMAPPING=ascii :are available on this server",
        )
        .await?;
        Ok(())
    }

    /// Common numeric reply.
    async fn numeric_reply<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: NumericReply,
        message: S,
    ) -> Result<()> {
        // :<source> <number> <client> :<message>
        self.stream
            .write_all(
                format!(
                    ":{} {} {} :{}\r\n",
                    self.server_addr.ip(),
                    number.to_string(),
                    client.username,
                    message.as_ref()
                )
                .as_bytes(),
            )
            .await
    }

    /// Numeric reply without the trailer marker.
    async fn numeric_reply_notrailer<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: NumericReply,
        message: S,
    ) -> Result<()> {
        // :<source> <number> <client> <message>
        self.stream
            .write_all(
                format!(
                    ":{} {} {} {}\r\n",
                    self.server_addr.ip(),
                    number.to_string(),
                    client.username,
                    message.as_ref()
                )
                .as_bytes(),
            )
            .await
    }

    /// Sends a notice to the client.
    #[allow(dead_code)]
    async fn notice<S: AsRef<str>>(&mut self, client: &ClientInfo, message: S) -> Result<()> {
        self.stream
            .write_all(format!("NOTICE {} :{}\r\n", client.username, message.as_ref()).as_bytes())
            .await?;
        Ok(())
    }

    /// Sends the PONG command.
    async fn pong(&mut self) -> Result<()> {
        self.stream
            .write_all(format!("PONG {}\r\n", self.server_addr.ip()).as_bytes())
            .await?;
        Ok(())
    }

    /// Sends an ERROR command with a custom message.
    async fn error<S: AsRef<str>>(&mut self, message: S) -> Result<()> {
        self.stream
            .write_all(format!("ERROR :{}", message.as_ref()).as_bytes())
            .await?;
        Ok(())
    }

    /// Sends a KILL command.
    async fn quit(&mut self, client: &ClientInfo) -> Result<()> {
        self.stream
            .write_all(
                format!(":{} QUIT :Quit: Server shutting down\r\n", client.username).as_bytes(),
            )
            .await?;
        Ok(())
    }
}

/// Threaded client loop
async fn handle_client(stream: TcpStream, mut rx: Receiver<()>) -> Result<()> {
    // In-memory database :^)
    let mut client_info = ClientInfo::default();

    let client_addr = stream.peer_addr().expect("Client had no address.");
    let server_addr = stream.local_addr().expect("Server had no address.");
    println!("New connection from {:?}", client_addr);

    let (read, write) = stream.into_split();

    let mut reader = BufReader::new(read);
    let mut writer = IrcWriter::new(write, server_addr, client_addr);

    let mut buf = String::new();
    loop {
        tokio::select! {
            _ = rx.changed() => {
                // Exit immediately
                writer.quit(&client_info).await?;
                writer.error("Server shutting down!").await?;
                return Ok(())
            }
            r = reader.read_line(&mut buf) => {
                match r {
                    Ok(u) => {
                        if u == 0 {
                            println!(
                                "Client {:?} gracefully closed connection with EOF.",
                                client_addr
                            );
                            return Ok(());
                        }
                        let parts: Vec<&str> = buf.split_ascii_whitespace().collect();

                        let command = parts[0].to_uppercase();

                        match command.as_str() {
                            "NICK" => {
                                client_info.nickname = parts[1].to_string();
                                println!("Received nickname: {:?}", client_info);
                            }
                            "USER" => {
                                client_info.username = parts[1].to_string();
                                client_info.realname = buf
                                    .split(':')
                                    .last()
                                    .expect("No real name provided")
                                    .to_string();
                                println!("Received user registration: {:?}", client_info);
                                writer.registration_reply(&client_info).await?;
                            }
                            "PING" => {
                                println!("Received ping, sending pong.");
                                writer.pong().await?;
                            }
                            "MOTD" => {
                                println!("{} wants a MOTD!!!!!", client_addr);
                                writer.motd(&client_info).await?;
                            }
                            "QUIT" => {
                                println!("Client said goodbye! {}", client_addr);
                                writer.error("Goodbye!").await?;
                                return Ok(());
                            }
                            "MODE" => {
                                println!("Ignoring MODE.");
                            }
                            _ => {
                                println!("Recieved unknown command: {:?}", parts);
                                writer
                                    .numeric_reply_notrailer(
                                        &client_info,
                                        NumericReply::ERR_UNKNOWN_COMMAND,
                                        format!("* {}: Unknown Command", parts[0]),
                                    )
                                    .await?;
                            }
                        };
                    }
                    Err(e) => {
                        println!("Client disconnected badly, encountered IO error {}", e);
                        return Ok(());
                    }
                }
            }
        }

        buf.clear();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:6667").await?;

    println!("Listening on {:?}", listener.local_addr());

    let killers: Arc<Mutex<Vec<Sender<()>>>> = Arc::new(Mutex::new(Vec::new()));

    let thread_killers = Arc::clone(&killers);
    tokio::spawn(async move {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for ctrl-c");
        // This may or may not ever happen
        println!("Ctrl-C received, terminating");
        for tx in thread_killers.lock().unwrap().iter() {
            tx.send(()).unwrap();
        }
        // Wait 100ms and assume that all clients have been killed
        // This should actually wait for client handling threads to reply that they have sent the messages
        // but I can't figure out the lifetimes so fuck it
        tokio::time::sleep(Duration::from_millis(100)).await;
        exit(0);
    });

    loop {
        let (stream, _) = listener.accept().await?;

        let (tx, rx) = watch::channel(());
        killers.lock().unwrap().push(tx);

        tokio::spawn(handle_client(stream, rx));
    }
}
