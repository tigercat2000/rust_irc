use crate::{ClientConnection, Result};
use std::str::FromStr;

type Tag = String;
type Source = String;
type Parameter = String;

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug, Clone)]
pub enum CommandType {
    NICK,
    USER,
    PING,
    MOTD,
    QUIT,
    PRIVMSG,
    JOIN,
    UNKNOWN(String),
}

impl FromStr for CommandType {
    type Err = Box<dyn std::error::Error + Send + Sync>;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "NICK" => Ok(Self::NICK),
            "USER" => Ok(Self::USER),
            "PING" => Ok(Self::PING),
            "MOTD" => Ok(Self::MOTD),
            "QUIT" => Ok(Self::QUIT),
            "PRIVMSG" => Ok(Self::PRIVMSG),
            "JOIN" => Ok(Self::JOIN),
            _ => {
                eprintln!("UNKNOWN Command: {:?}", s.to_uppercase());
                Ok(Self::UNKNOWN(s.to_uppercase()))
            }
        }
    }
}

impl ToString for CommandType {
    fn to_string(&self) -> String {
        match self {
            CommandType::NICK => "NICK",
            CommandType::USER => "USER",
            CommandType::PING => "PING",
            CommandType::MOTD => "MOTD",
            CommandType::QUIT => "QUIT",
            CommandType::PRIVMSG => "PRIVMSG",
            CommandType::JOIN => "JOIN",
            CommandType::UNKNOWN(x) => x,
        }
        .to_string()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Side {
    Server,
    Client,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Command {
    pub tags: Vec<Tag>,
    pub source: Option<Source>,
    pub command: CommandType,
    pub parameters: Vec<Parameter>,
    // Metadata
    pub side: Side,
}

impl ToString for Command {
    fn to_string(&self) -> String {
        let mut str = String::new();
        if !self.tags.is_empty() {
            str.push_str(&self.tags.join(" "));
            str.push(' ');
        }
        if let Some(x) = &self.source {
            str.push_str(x);
            str.push(' ');
        }
        str.push_str(&self.command.to_string());
        str.push(' ');

        str.push_str(&self.parameters.join(" "));

        str.push_str("\r\n");
        str
    }
}

#[derive(Debug)]
pub enum Code {
    Fine,
    Broadcast,
    Exit,
}

impl Command {
    pub fn parse<S: AsRef<str>>(frame: S, side: Side) -> Result<Self> {
        let str = frame.as_ref();
        let parts = str
            .split(' ')
            .map(|x| x.to_string())
            .collect::<Vec<String>>();

        let mut source = None;
        let command;
        let parameters_no_trailer;

        if parts[0].starts_with(':') {
            source = Some(parts[0].clone());
            command = CommandType::from_str(parts[1].trim())?;
            parameters_no_trailer = parts[2..].to_vec();
        } else {
            command = CommandType::from_str(parts[0].trim())?;
            parameters_no_trailer = parts[1..].to_vec();
        }

        let mut parameters = Vec::new();
        for (i, x) in parameters_no_trailer.iter().enumerate() {
            if x.starts_with(':') {
                parameters.push(parameters_no_trailer[i..].join(" ").trim().to_string());
                break;
            }
            parameters.push(x.trim().to_string());
        }

        Ok(Self {
            tags: Vec::new(),
            source,
            command,
            parameters,
            side,
        })
    }

    pub async fn apply(&self, cc: &mut ClientConnection) -> Result<Code> {
        match &self.command {
            CommandType::NICK => {
                cc.info.nickname = self.parameters.first().unwrap().to_string();
            }
            CommandType::USER => {
                if self.parameters.len() != 4 {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "USER command had the wrong amount of parameters",
                    )));
                }
                cc.info.username = self.parameters[0].to_string();
                cc.info.realname = self.parameters[3].to_string();
                cc.connection.write_registration(&cc.info).await?;
            }
            CommandType::PING => {
                println!(
                    "[{}] PING detected, writing PONG.",
                    cc.connection.client_addr.ip(),
                );
                cc.connection.write_pong(&self.parameters[0]).await?;
            }
            CommandType::MOTD => {
                cc.connection.write_motd(&cc.info).await?;
            }
            CommandType::QUIT => {
                cc.connection.write_error("Goodbye!").await?;
                return Ok(Code::Exit);
            }
            CommandType::PRIVMSG => match self.side {
                Side::Client => return Ok(Code::Broadcast),
                // Safety: self.to_string() always ends with \r\n.
                Side::Server => unsafe {
                    let str = self.to_string();
                    println!("PRIVMSG broadcast, writing: {:?}", str);
                    cc.connection.write_raw(str).await?;
                },
            },
            CommandType::JOIN => match self.side {
                Side::Client => {
                    cc.info.channels.push(self.parameters[0].clone());
                    return Ok(Code::Broadcast);
                }
                Side::Server => {
                    // Safety: self.to_string() always ends with \r\n.
                    unsafe {
                        cc.connection.write_raw(self.to_string()).await?;
                    }
                }
            },
            CommandType::UNKNOWN(attempt) => {
                cc.connection.write_unknown(&cc.info, attempt).await?;
            }
        }
        Ok(Code::Fine)
    }
}
