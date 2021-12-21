use crate::{ClientConnection, Result};
use std::str::FromStr;

type Tag = String;
type Source = String;
type Parameter = String;

#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
#[derive(Debug)]
enum CommandType {
    NICK,
    USER,
    PING,
    MOTD,
    QUIT,
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
            _ => {
                eprintln!("UNKNOWN Command: {:?}", s.to_uppercase());
                Ok(Self::UNKNOWN(s.to_uppercase()))
            }
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Command {
    tags: Vec<Tag>,
    source: Option<Source>,
    command: CommandType,
    parameters: Vec<Parameter>,
}

#[derive(Debug)]
pub enum Code {
    Fine,
    Exit,
}

impl Command {
    pub fn parse<S: AsRef<str>>(frame: S) -> Result<Self> {
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
                parameters.push(parameters_no_trailer[i..].join(" "));
                break;
            }
            parameters.push(x.trim().to_string());
        }

        Ok(Self {
            tags: Vec::new(),
            source,
            command,
            parameters,
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
                cc.connection.write_pong().await?;
            }
            CommandType::MOTD => {
                cc.connection.write_motd(&cc.info).await?;
            }
            CommandType::QUIT => {
                cc.connection.write_error("Goodbye!").await?;
                return Ok(Code::Exit);
            }
            CommandType::UNKNOWN(attempt) => {
                cc.connection.write_unknown(&cc.info, attempt).await?;
            }
        }
        Ok(Code::Fine)
    }
}
