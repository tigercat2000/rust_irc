mod command;
use command::Command;
mod irc_connection;
use irc_connection::IrcConnection;
mod server;
use server::{ClientConnection, ClientInfo};
mod shutdown;
use shutdown::Shutdown;
use tokio::{net::TcpListener, signal};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("0.0.0.0:6667").await?;
    println!("Listening on {}", listener.local_addr().unwrap());
    server::run(listener, signal::ctrl_c()).await;
    Ok(())
}
