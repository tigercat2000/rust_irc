use std::io::{prelude::*, BufReader};
use std::net::{Shutdown, TcpListener, TcpStream};
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

/// Wrapper around TcpStream that handles common write operations for IRC traffic.
struct IrcWriter(TcpStream);

impl IrcWriter {
    /// Make a new IrcWriter for `stream`.
    fn new(stream: TcpStream) -> Self {
        Self(stream)
    }

    /// Common numeric_reply.
    fn numeric_reply<S: AsRef<str>, S2: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: S,
        message: S2,
    ) -> std::io::Result<()> {
        write!(
            self.0,
            ":{} {} {} {}\r\n",
            self.0.local_addr().expect("Server had no address.").ip(),
            number.as_ref(),
            client.username,
            message.as_ref()
        )
    }
}

/// Threaded client loop
fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
    // In-memory database :^)
    let mut client_info = ClientInfo::default();

    let client_addr = stream.peer_addr().expect("Client had no address.");
    println!("New connection from {:?}", client_addr);

    let server_addr = stream
        .local_addr()
        .expect("Server had no address.")
        .ip()
        .to_string();

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
                        writer.numeric_reply(
                            &client_info,
                            "001",
                            format!(
                                "{} :Welcome to the Internet Relay Network {}",
                                client_addr,
                                client_info.to_canonical(server_addr.as_str())
                            ),
                        )?;
                    }
                    "PING" => {
                        println!("Received ping, sending pong.");
                        stream.write_fmt(format_args!("PONG {}\r\n", server_addr))?;
                    }
                    "MOTD" => {
                        println!("{} wants a MOTD!!!!!", client_addr);
                        // RPL_MOTDSTART
                        writer.numeric_reply(
                            &client_info,
                            "375",
                            format!(":- {} Message of the day - ", server_addr),
                        )?;
                        // RPL_MOTD
                        writer.numeric_reply(&client_info, "372", ":- Hi from Rust-IRC!")?;
                        // RPL_ENDOFMOTD
                        writer.numeric_reply(&client_info, "376", ":End of /MOTD command")?;
                    }
                    "QUIT" => {
                        println!("Client said goodbye! {}", client_addr);
                        stream.write_fmt(format_args!("ERROR :{} Goodbye!\r\n", server_addr))?;
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
            Err(e) => panic!("IO error: {}", e),
        }
        buf.clear();
    }
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:6667")?;
    ctrlc::set_handler(|| {
        println!("Ctrl-C received, exiting.");
        exit(0);
    })
    .expect("Error setting Ctrl-C handler.");

    println!("Listening on {:?}", listener.local_addr());
    for stream in listener.incoming() {
        std::thread::spawn(move || handle_client(stream?));
    }
    Ok(())
}
