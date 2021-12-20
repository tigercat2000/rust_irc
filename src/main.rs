use std::io::{prelude::*, BufReader};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::ops::{Deref, DerefMut};
use std::process::exit;

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
}

impl ToString for NumericReply {
    fn to_string(&self) -> String {
        format!("{:0>3}", *self as usize)
    }
}

/// Wrapper around TcpStream that handles common write operations for IRC traffic.
struct IrcWriter(TcpStream);

impl IrcWriter {
    /// Make a new IrcWriter for `stream`.
    fn new(stream: TcpStream) -> Self {
        Self(stream)
    }

    /// Sends the numeric reply sequence for the MOTD.
    fn motd(&mut self, client: &ClientInfo) -> std::io::Result<()> {
        self.numeric_reply(
            client,
            NumericReply::RPL_MOTDSTART,
            format!("- {} Message of the day - ", self.0.local_addr()?.ip()),
        )?;
        self.numeric_reply(client, NumericReply::RPL_MOTD, "- Hi from Rust-IRC!")?;
        self.numeric_reply(client, NumericReply::RPL_ENDOFMOTD, "End of /MOTD command")?;
        Ok(())
    }

    /// This is the 5 packet series required after a registration has finished.
    fn registration_reply(&mut self, client: &ClientInfo) -> std::io::Result<()> {
        self.numeric_reply(
            client,
            NumericReply::RPL_WELCOME,
            format!(
                "Welcome to the Internet Relay Network {}",
                client.to_canonical(self.0.local_addr()?.ip().to_string())
            ),
        )?;
        self.numeric_reply(
            client,
            NumericReply::RPL_YOURHOST,
            format!(
                "Your host is {}, running version rust_irc-0.0.0",
                self.0.local_addr()?.ip()
            ),
        )?;
        self.numeric_reply(
            client,
            NumericReply::RPL_CREATED,
            "This server was created... probably 10 seconds ago who cares",
        )?;
        self.numeric_reply_notrailer(
            client,
            NumericReply::RPL_MYINFO,
            format!(
                "{} {} {} {}",
                self.0.local_addr()?.ip(),
                "rust_irc-0.0.0",
                " ",
                " "
            ),
        )?;
        self.numeric_reply_notrailer(
            client,
            NumericReply::RPL_ISUPPORT,
            "CASEMAPPING=ascii :are available on this server",
        )?;
        Ok(())
    }

    /// Common numeric reply.
    fn numeric_reply<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: NumericReply,
        message: S,
    ) -> std::io::Result<()> {
        // :<source> <number> <client> :<message>
        write!(
            self.0,
            ":{} {} {} :{}\r\n",
            self.0.local_addr()?.ip(),
            number.to_string(),
            client.username,
            message.as_ref()
        )
    }

    /// Numeric reply without the trailer marker.
    fn numeric_reply_notrailer<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: NumericReply,
        message: S,
    ) -> std::io::Result<()> {
        // :<source> <number> <client> <message>
        write!(
            self.0,
            ":{} {} {} {}\r\n",
            self.0.local_addr()?.ip(),
            number.to_string(),
            client.username,
            message.as_ref()
        )
    }

    /// Sends a notice to the client.
    #[allow(dead_code)]
    fn notice<S: AsRef<str>>(&mut self, client: &ClientInfo, message: S) -> std::io::Result<()> {
        write!(
            self.0,
            "NOTICE {} :{}\r\n",
            client.username,
            message.as_ref()
        )
    }

    /// Sends the PONG command.
    fn pong(&mut self) -> std::io::Result<()> {
        write!(self.0, "PONG {}\r\n", self.0.local_addr()?.ip())
    }

    /// Sends an ERROR command with a custom message.
    fn error<S: AsRef<str>>(&mut self, message: S) -> std::io::Result<()> {
        write!(self.0, "ERROR :{}", message.as_ref())
    }
}

struct ClientStreamWrapper(TcpStream);

impl Drop for ClientStreamWrapper {
    fn drop(&mut self) {
        let mut writer = IrcWriter::new(self.0.try_clone().expect("Failed to clean up stream."));
        writer.error("Server shutting down.").unwrap();
    }
}

impl Deref for ClientStreamWrapper {
    type Target = TcpStream;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ClientStreamWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Threaded client loop
fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    // In-memory database :^)
    let mut client_info = ClientInfo::default();

    let mut stream = ClientStreamWrapper(stream);

    let client_addr = stream.peer_addr().expect("Client had no address.");
    println!("New connection from {:?}", client_addr);

    let mut reader = BufReader::new(
        stream
            .try_clone()
            .expect("Failed to separate reader from stream."),
    );

    let mut writer = IrcWriter::new(
        stream
            .try_clone()
            .expect("Failed to separate writer from stream."),
    );

    let mut buf = String::new();
    loop {
        match reader.read_line(&mut buf) {
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
                        writer.registration_reply(&client_info)?;
                    }
                    "PING" => {
                        println!("Received ping, sending pong.");
                        writer.pong()?;
                    }
                    "MOTD" => {
                        println!("{} wants a MOTD!!!!!", client_addr);
                        writer.motd(&client_info)?;
                    }
                    "QUIT" => {
                        println!("Client said goodbye! {}", client_addr);
                        writer.error("Goodbye!")?;
                        stream.shutdown(Shutdown::Both)?;
                        return Ok(());
                    }
                    "MODE" => {
                        println!("Ignoring MODE.");
                    }
                    _ => {
                        println!("Recieved unknown command: {:?}", parts);
                        stream.write_fmt(format_args!(
                            ":{} 421 * {}: Unknown command\r\n",
                            client_addr, parts[0]
                        ))?;
                    }
                };
            }
            Err(e) => {
                println!("Client disconnected badly, encountered IO error {}", e);
                stream.shutdown(Shutdown::Both)?;
                return Ok(());
            }
        }
        buf.clear();
    }
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:6667")?;

    println!("Listening on {:?}", listener.local_addr());
    for stream in listener.incoming() {
        std::thread::spawn(move || handle_client(stream?));
    }

    ctrlc::set_handler(|| {
        println!("Ctrl-C received, exiting.");
        exit(0);
    })
    .expect("Error setting Ctrl-C handler.");

    Ok(())
}
