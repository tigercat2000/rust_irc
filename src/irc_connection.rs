macro_rules! format_write {
    ($dst:expr, $($arg:tt)*) => {
        $dst.write_all(format!($($arg)*).as_bytes()).await?;
        $dst.flush().await?;
    };
}

use crate::{ClientInfo, Result};
use std::net::SocketAddr;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::TcpStream,
};

#[repr(usize)]
#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types, dead_code)]
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

#[derive(Debug)]
#[allow(dead_code)]
pub struct IrcConnection {
    client_addr: SocketAddr,
    server_addr: SocketAddr,
    stream: BufWriter<BufReader<TcpStream>>,
}

// Wrapper stuff.
impl IrcConnection {
    /// Creates a new IrcConnection wrapper with buffered read/write over the socket.
    pub fn new(socket: TcpStream) -> Self {
        Self {
            client_addr: socket.peer_addr().expect("Client didn't have an address."),
            server_addr: socket.local_addr().expect("Server didn't have an address."),
            stream: BufWriter::new(BufReader::new(socket)),
        }
    }

    /// Reads a line if possible, or exits if the stream has closed.
    pub async fn read_line(&mut self) -> Result<Option<String>> {
        let mut buf = String::new();
        if 0 == self.stream.read_line(&mut buf).await? {
            return Ok(None);
        }
        Ok(Some(buf))
    }
}

// Private helpers for writing IRC commands to the stream.
impl IrcConnection {
    async fn write_numeric<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: NumericReply,
        message: S,
    ) -> Result<()> {
        let username = if client.username.is_empty() {
            "*"
        } else {
            client.username.as_str()
        };
        format_write!(
            self.stream,
            ":{} {} {} {}\r\n",
            self.server_addr.ip(),
            number.to_string(),
            username,
            message.as_ref()
        );
        println!(
            "Writing {:?}",
            format!(
                ":{} {} {} {}\r\n",
                self.server_addr.ip(),
                number.to_string(),
                username,
                message.as_ref()
            )
        );
        Ok(())
    }

    async fn write_numeric_trailer<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        number: NumericReply,
        message: S,
    ) -> Result<()> {
        self.write_numeric(client, number, format!(":{}", message.as_ref()))
            .await?;
        Ok(())
    }
}

// Actual IRC commands.
impl IrcConnection {
    pub async fn write_quit<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        reason: S,
    ) -> Result<()> {
        format_write!(
            self.stream,
            ":{} QUIT :{}\r\n",
            client.username,
            reason.as_ref()
        );
        Ok(())
    }

    pub async fn write_error<S: AsRef<str>>(&mut self, error: S) -> Result<()> {
        format_write!(self.stream, "ERROR :{}", error.as_ref());
        Ok(())
    }

    pub async fn write_registration(&mut self, client: &ClientInfo) -> Result<()> {
        println!("Writing registration");
        self.write_numeric_trailer(
            client,
            NumericReply::RPL_WELCOME,
            format!(
                "Welcome to the Internet Relay Network {}",
                client.to_canonical(self.server_addr.ip().to_string())
            ),
        )
        .await?;
        self.write_numeric_trailer(
            client,
            NumericReply::RPL_YOURHOST,
            format!(
                "Your host is {}, running version rust_irc-0.0.0",
                self.server_addr.ip()
            ),
        )
        .await?;
        self.write_numeric_trailer(
            client,
            NumericReply::RPL_CREATED,
            "This server was created... probably 10 seconds ago who cares",
        )
        .await?;
        self.write_numeric(
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
        self.write_numeric(
            client,
            NumericReply::RPL_ISUPPORT,
            "CASEMAPPING=ascii :are available on this server",
        )
        .await?;
        Ok(())
    }

    pub async fn write_unknown<S: AsRef<str>>(
        &mut self,
        client: &ClientInfo,
        command: S,
    ) -> Result<()> {
        self.write_numeric(
            client,
            NumericReply::ERR_UNKNOWN_COMMAND,
            format!("* {}: Unknown command", command.as_ref()),
        )
        .await?;
        Ok(())
    }

    pub async fn write_pong(&mut self) -> Result<()> {
        format_write!(self.stream, "PONG {}", self.server_addr.ip());
        Ok(())
    }

    pub async fn write_motd(&mut self, client: &ClientInfo) -> Result<()> {
        self.write_numeric(
            client,
            NumericReply::RPL_MOTDSTART,
            format!("- {} Message of the day - ", self.server_addr.ip()),
        )
        .await?;
        self.write_numeric(client, NumericReply::RPL_MOTD, "- Hi from Rust-IRC!")
            .await?;
        self.write_numeric(client, NumericReply::RPL_MOTD, "End of /MOTD command")
            .await?;
        Ok(())
    }
}
