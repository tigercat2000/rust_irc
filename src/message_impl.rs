use crate::message_parse::{Command, Message, Side};
use crate::ClientConnection;
use crate::Result;

#[derive(Debug)]
pub enum Code {
    Fine,
    Broadcast,
    Exit,
}

impl Message {
    pub async fn apply(&self, cc: &mut ClientConnection) -> Result<Code> {
        match &self.command {
            Command::NICK(nickname) => {
                cc.info.nickname = nickname.clone();
            }
            Command::USER(username, _, _, realname) => {
                cc.info.username = username.clone();
                cc.info.realname = realname.clone();
                cc.connection.write_registration(&cc.info).await?;
            }
            Command::PING(token) => {
                cc.connection.write_pong(token).await?;
            }
            Command::MOTD(_) => {
                cc.connection.write_motd(&cc.info).await?;
            }
            Command::QUIT(_reason) => {
                cc.connection.write_error("Goodbye!").await?;
                return Ok(Code::Exit);
            }
            Command::PRIVMSG(_targets, _message) => match self.side {
                Side::Client => return Ok(Code::Broadcast),
                // Safety: self.to_string() always ends with \r\n.
                Side::Server => unsafe {
                    let str = self.to_string();
                    cc.connection.write_raw(str).await?;
                },
                _ => {}
            },
            Command::JOIN(targets, _keys) => match self.side {
                Side::Client => {
                    for chan in targets {
                        cc.info.channels.push(chan.clone());
                    }
                    // We have to parrot the client's JOIN back to them.
                    // Safety: self.to_string() always ends with \r\n
                    unsafe {
                        cc.connection.write_raw(self.to_string()).await?;
                    }
                    return Ok(Code::Broadcast);
                }
                Side::Server => {
                    // Safety: self.to_string() always ends with \r\n.
                    unsafe {
                        cc.connection.write_raw(self.to_string()).await?;
                    }
                }
                _ => {}
            },
            Command::UNKNOWN(attempt) | Command::UNIMPLEMENTED(attempt) => {
                cc.connection.write_unknown(&cc.info, attempt).await?;
            }
            _ => {}
        }
        Ok(Code::Fine)
    }
}
