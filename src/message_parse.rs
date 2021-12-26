use std::str::FromStr;

type Target = String;
type Nickname = String;
type Channel = String;
type Msg = String;
type Server = String;
type Port = String;
type Subcommand = String;
type Key = String;
type ServerMask = String;
type Password = String;
type Token = String;
type Query = String;
type UserMode = String;
type ModeString = String;
type Unused = String;
type NicknameMask = String;
type Username = String;
type Realname = String;

fn minlength_or_fail(x: &[&str], len: usize) -> std::result::Result<(), std::io::Error> {
    if x.len() < len {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid length",
        ))
    } else {
        Ok(())
    }
}

fn strip_colon(mut a: String) -> std::result::Result<String, std::io::Error> {
    if a.is_empty() {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid length for colon stripping",
        ))
    } else {
        // Panic handling: chars().next().unwrap() will never panic if there's at least one character in the string
        // which is required for is_empty() to fail.
        if a.starts_with(':') {
            a.remove(0);
        }
        // If the colon was the only thing there, we have an invalid input.
        if a.is_empty() {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "No content after colon",
            ))
        } else {
            Ok(a)
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
// It's a protocol spec, we follow it
#[allow(clippy::upper_case_acronyms)]
pub enum Command {
    ADMIN(Option<Target>),
    AWAY(Option<Msg>),
    // CNOTICE(Nickname, Channel, Msg),
    // CPRIVMSG(Nickname, Channel, Msg),
    CONNECT(Server, Port, Server),
    DIE,
    ENCAP(Server, Subcommand, Vec<String>),
    ERROR(Msg),
    HELP,
    INFO(Option<Target>),
    INVITE(Nickname, Channel),
    // ISON(Vec<Nickname>),
    JOIN(Vec<Channel>, Option<Vec<Key>>),
    KICK(Channel, Nickname, Option<Msg>),
    KILL(Nickname, Msg),
    KNOCK(Channel, Option<Msg>),
    LINKS(Option<Server>, Option<ServerMask>),
    LIST(Option<Vec<Channel>>, Option<Server>),
    LUSERS(Option<ServerMask>, Option<Server>),
    MODE(Target, Option<ModeString>, Option<Vec<String>>),
    MOTD(Option<Server>),
    NAMES(Option<Vec<Channel>>),
    // NAMESX,
    NICK(Nickname),
    NOTICE(Vec<Target>, Msg),
    OPER(Nickname, Password),
    PART(Vec<Channel>, Msg),
    PASS(Password),
    PING(Token),
    PONG(Server, Token),
    PRIVMSG(Vec<Target>, Msg),
    QUIT(Option<Msg>),
    REHASH,
    // RULES,
    // SERVER(),
    // SERVICE,
    // SERVLIST,
    // SQUERY,
    SQUIT(Option<Server>, Msg),
    // SETNAME,
    // SILENCE,
    STATS(Query, Option<Server>),
    // SUMMON,
    TIME(Option<Server>),
    TOPIC(Channel, Option<Msg>),
    TRACE(Option<Target>),
    // UHNAMES,
    USER(Username, UserMode, Unused, Realname),
    USERHOST(Vec<Nickname>),
    USERIP(Nickname),
    USERS(Option<Server>),
    VERSION(Option<Server>),
    WALLOPS(Msg),
    // WATCH,
    WHO(NicknameMask),
    WHOIS(Option<Target>, Nickname),
    // WHOWAS,
    /// We have no fucking idea what garbage we just got
    UNKNOWN(String),
    /// We know this is actually valid, we just don't support it yet.
    UNIMPLEMENTED(String),
}

impl FromStr for Command {
    type Err = std::io::Error;
    /// Takes everything past the `<command>` part of the IRC standard:
    /// ```bnf
    /// message ::= ['@' <tags> SPACE] [':' <source> SPACE] <command> <parameters> <crlf>
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // This can never be empty, be s.split() will always return at least one element.
        let parts: Vec<&str> = s.split(' ').collect();

        let message = match parts[0].to_uppercase().as_str() {
            "DIE" => Self::DIE,
            "JOIN" => {
                // Need at least one channel.
                minlength_or_fail(&parts, 2)?;
                let channels = parts[1]
                    .split(',')
                    .map(|x| x.to_string())
                    .collect::<Vec<String>>();
                let mut keys = None;
                if parts.len() == 3 {
                    keys = Some(
                        parts[2]
                            .split(',')
                            .map(|x| x.to_string())
                            .collect::<Vec<String>>(),
                    );
                }
                Self::JOIN(channels, keys)
            }
            "MOTD" => {
                if parts.len() != 1 {
                    Self::UNIMPLEMENTED(s.trim().to_string())
                } else {
                    Self::MOTD(None)
                }
            }
            "NICK" => {
                minlength_or_fail(&parts, 2)?;
                // Spaces aren't allowed.
                Self::NICK(parts[1].to_string())
            }
            "PING" => {
                minlength_or_fail(&parts, 2)?;
                Self::PING(parts[1].to_string())
            }
            "PONG" => {
                minlength_or_fail(&parts, 3)?;
                Self::PONG(parts[1].to_string(), parts[2].to_string())
            }
            "PRIVMSG" => {
                minlength_or_fail(&parts, 3)?;
                // The first parameter is comma separated targets
                let targets = parts[1];
                // The rest of the command is the message.
                let message = strip_colon(
                    parts[2..]
                        .iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>()
                        .join(" "),
                )?;
                Self::PRIVMSG(
                    targets
                        .split(',')
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>(),
                    message,
                )
            }
            "QUIT" => {
                let mut message = None;
                if let Some(pieces) = parts.get(1..) {
                    if !pieces.is_empty() && !pieces[0].is_empty() {
                        message = Some(strip_colon(pieces.join(" "))?);
                    }
                }
                Self::QUIT(message)
            }
            "REHASH" => Self::REHASH,
            "USER" => {
                minlength_or_fail(&parts, 5)?;
                let realname = strip_colon(parts[4..].join(" "))?;
                Self::USER(
                    parts[1].to_string(),
                    parts[2].to_string(),
                    parts[3].to_string(),
                    realname,
                )
            }
            // Yep, split() can do this to us.
            "" => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Blank input",
                ));
            }
            _ => Self::UNKNOWN(s.trim().to_string()),
        };
        eprintln!("Message parsed: {:?}", message);
        Ok(message)
    }
}

impl ToString for Command {
    fn to_string(&self) -> String {
        let str = match self {
            Command::ADMIN(_) => todo!(),
            Command::AWAY(_) => todo!(),
            Command::CONNECT(_, _, _) => todo!(),
            Command::DIE => "DIE".to_string(),
            Command::ENCAP(_, _, _) => todo!(),
            Command::ERROR(_) => todo!(),
            Command::HELP => todo!(),
            Command::INFO(_) => todo!(),
            Command::INVITE(_, _) => todo!(),
            Command::JOIN(chans, maybe_keys) => {
                if let Some(keys) = maybe_keys {
                    format!("JOIN {} {}", chans.join(","), keys.join(","))
                } else {
                    format!("JOIN {}", chans.join(","))
                }
            }
            Command::KICK(_, _, _) => todo!(),
            Command::KILL(_, _) => todo!(),
            Command::KNOCK(_, _) => todo!(),
            Command::LINKS(_, _) => todo!(),
            Command::LIST(_, _) => todo!(),
            Command::LUSERS(_, _) => todo!(),
            Command::MODE(_, _, _) => todo!(),
            Command::MOTD(x) if x.is_some() => todo!(),
            Command::MOTD(_) => "MOTD".to_string(),
            Command::NAMES(_) => todo!(),
            Command::NICK(nickname) => format!("NICK {}", nickname),
            Command::NOTICE(_, _) => todo!(),
            Command::OPER(_, _) => todo!(),
            Command::PART(_, _) => todo!(),
            Command::PASS(_) => todo!(),
            Command::PING(token) => format!("PING {}", token),
            Command::PONG(server, token) => format!("PONG {} {}", server, token),
            Command::PRIVMSG(targets, message) => {
                format!("PRIVMSG {} :{}", targets.join(","), message)
            }
            Command::QUIT(maybe_reason) => {
                if let Some(reason) = maybe_reason {
                    format!("QUIT :{}", reason)
                } else {
                    "QUIT".to_string()
                }
            }
            Command::REHASH => "REHASH".to_string(),
            Command::SQUIT(_, _) => todo!(),
            Command::STATS(_, _) => todo!(),
            Command::TIME(_) => todo!(),
            Command::TOPIC(_, _) => todo!(),
            Command::TRACE(_) => todo!(),
            Command::USER(username, mode, un, real) => {
                if real.contains(' ') {
                    format!("USER {} {} {} :{}", username, mode, un, real)
                } else {
                    format!("USER {} {} {} {}", username, mode, un, real)
                }
            }
            Command::USERHOST(_) => todo!(),
            Command::USERIP(_) => todo!(),
            Command::USERS(_) => todo!(),
            Command::VERSION(_) => todo!(),
            Command::WALLOPS(_) => todo!(),
            Command::WHO(_) => todo!(),
            Command::WHOIS(_, _) => todo!(),
            Command::UNKNOWN(s) => s.clone(),
            Command::UNIMPLEMENTED(s) => s.clone(),
        };
        eprintln!("Message stringified: {:?}", str);
        str
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Side {
    Client,
    Server,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Message {
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) source: Option<String>,
    pub(crate) command: Command,
    pub(crate) side: Side,
}

impl FromStr for Message {
    type Err = std::io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut new_self = Self {
            tags: None,
            source: None,
            command: Command::UNKNOWN("".to_string()),
            side: Side::Unknown,
        };

        let mut rest = "".to_string();

        let parts = s.split(' ').collect::<Vec<&str>>();

        // Tags
        match (
            parts[0].starts_with('@'),
            parts[0].starts_with(':'),
            parts[1].starts_with(':'),
        ) {
            // Not possible
            (true, true, true) => unreachable!(),
            // Not possible
            (true, true, false) => unreachable!(),
            // Tags and source
            (true, false, true) => {
                new_self.tags = Some(
                    parts[0][1..]
                        .split(';')
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>(),
                );
                new_self.source = Some(parts[1][1..].to_string());
                rest = parts[2..].join(" ");
            }
            // Tags, but no source
            (true, false, false) => {
                new_self.tags = Some(
                    parts[0]
                        .split(';')
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>(),
                );
            }
            // Reachable but invalid
            (false, true, true) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Messages can't start with multiple : in a row",
                ));
            }
            // Source, but no tags
            (false, true, false) => {
                new_self.source = Some(parts[0][1..].to_string());
                rest = parts[1..].join(" ");
            }
            // Some one-parameter command
            (false, false, true) => {
                rest = parts.join(" ");
            }
            // Some x-parameter command
            (false, false, false) => {
                rest = parts.join(" ");
            }
        }

        new_self.command = Command::from_str(&rest)?;
        Ok(new_self)
    }
}

impl ToString for Message {
    fn to_string(&self) -> String {
        match (&self.tags, &self.source) {
            (None, None) => self.command.to_string(),
            (None, Some(source)) => format!(":{} {}", source, self.command.to_string()),
            (Some(tags), None) => format!("@{} {}", tags.join(";"), self.command.to_string()),
            (Some(tags), Some(source)) => format!(
                "@{} :{} {}",
                tags.join(";"),
                source,
                self.command.to_string()
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn parse_privmessage() {
        let command: Command = "PRIVMSG #meow :Hi there".parse().unwrap();
        assert_eq!(
            command,
            Command::PRIVMSG(vec!["#meow".to_string()], "Hi there".to_string())
        );
    }

    #[test]
    fn parse_ping() {
        let command: Command = "PING wuiobgv9".parse().unwrap();
        assert_eq!(command, Command::PING("wuiobgv9".to_string()));
    }

    #[test]
    fn parse_join() {
        let command: Command = "JOIN #meow".parse().unwrap();
        assert_eq!(command, Command::JOIN(vec!["#meow".to_string()], None));
    }

    #[test]
    fn parse_join_key() {
        let command: Command = "JOIN #meow nyaa".parse().unwrap();
        assert_eq!(
            command,
            Command::JOIN(vec!["#meow".to_string()], Some(vec!["nyaa".to_string()]))
        );
    }

    #[test]
    fn parse_multi_join() {
        let command: Command = "JOIN #meow,#blep nyaa,mlem".parse().unwrap();
        assert_eq!(
            command,
            Command::JOIN(
                vec!["#meow".to_string(), "#blep".to_string()],
                Some(vec!["nyaa".to_string(), "mlem".to_string()])
            )
        );
    }

    #[test]
    fn parse_pong() {
        let command: Command = "PONG tigercat2000.dev wuiobgv9".parse().unwrap();
        assert_eq!(
            command,
            Command::PONG("tigercat2000.dev".to_string(), "wuiobgv9".to_string())
        );
    }

    #[test]
    fn parse_die() {
        let command: Command = "DIE".parse().unwrap();
        assert_eq!(command, Command::DIE);
    }

    #[test]
    fn parse_rehash() {
        let command: Command = "REHASH".parse().unwrap();
        assert_eq!(command, Command::REHASH);
    }

    #[test]
    fn parse_motd() {
        let command: Command = "MOTD".parse().unwrap();
        assert_eq!(command, Command::MOTD(None));
    }

    #[test]
    fn parse_quit() {
        let command: Command = "QUIT".parse().unwrap();
        assert_eq!(command, Command::QUIT(None));
    }

    #[test]
    fn parse_quit_with_message() {
        let command: Command = "QUIT :Leaving".parse().unwrap();
        assert_eq!(command, Command::QUIT(Some("Leaving".to_string())));
    }

    #[test]
    fn parse_user() {
        let command: Command = "USER guest 0 * :Meow Tompski".parse().unwrap();
        assert_eq!(
            command,
            Command::USER(
                "guest".to_string(),
                "0".to_string(),
                "*".to_string(),
                "Meow Tompski".to_string()
            )
        );
    }

    #[test]
    fn parse_nospace_realname() {
        let command: Command = "USER guest 0 * hola".parse().unwrap();
        assert_eq!(
            command,
            Command::USER(
                "guest".to_string(),
                "0".to_string(),
                "*".to_string(),
                "hola".to_string()
            )
        );
    }

    #[test]
    fn parse_malformed_user() {
        let command: Result<Command, std::io::Error> = "USER guest 0 * :".parse();
        match command {
            // Correct
            Err(ref e) if e.kind() == std::io::ErrorKind::InvalidInput => {}
            // Incorrect
            Ok(_) => panic!("Malformed USER command parsed anyways"),
            // Incorrect
            Err(e) => panic!("Malformed USER command incorrectly errored {}", e),
        }
    }

    #[test]
    fn parse_nonsense() {
        let command: Command = "POST / HTTP/1.1".parse().unwrap();
        assert_eq!(command, Command::UNKNOWN("POST / HTTP/1.1".to_string()));
    }

    #[test]
    fn parse_blank_string() {
        let command: Result<Command, std::io::Error> = "".parse();
        match command {
            // Correct
            Err(ref e) if e.kind() == std::io::ErrorKind::InvalidInput => {}
            // Incorrect
            Ok(_) => panic!("Malformed command parsed anyways"),
            // Incorrect
            Err(e) => panic!("Malformed command incorrectly errored {}", e),
        }
    }

    #[test]
    fn parse_adv_motd() {
        let command: Result<Command, std::io::Error> = "MOTD otherserver.com".parse();
        match command {
            // Correct
            Ok(Command::UNIMPLEMENTED(x)) if x == "MOTD otherserver.com" => {}
            // Incorrect
            Ok(_) => panic!("Malformed command parsed incorrectly"),
            // Incorrect
            Err(e) => panic!("Malformed command incorrectly errored {}", e),
        }
    }

    #[test]
    fn parse_full_ass_message() {
        let message: Message =
            "@meow;mlem :irc.example.com CAP LS * :multi-prefix extended-join sasl"
                .parse()
                .unwrap();
        assert_eq!(
            message,
            Message {
                tags: Some(vec!["meow".to_string(), "mlem".to_string()]),
                source: Some("irc.example.com".to_string()),
                command: Command::UNKNOWN("CAP LS * :multi-prefix extended-join sasl".to_string()),
                side: Side::Unknown,
            }
        )
    }

    #[test]
    fn parse_full_ass_message2() {
        let message: Message = "@meow;mlem :irc.example.com USER guest 0 * :Meow Tompski"
            .parse()
            .unwrap();
        assert_eq!(
            message,
            Message {
                tags: Some(vec!["meow".to_string(), "mlem".to_string()]),
                source: Some("irc.example.com".to_string()),
                command: Command::USER(
                    "guest".to_string(),
                    "0".to_string(),
                    "*".to_string(),
                    "Meow Tompski".to_string()
                ),
                side: Side::Unknown,
            }
        )
    }

    #[test]
    fn test_to_string_matches_from_string() {
        let mut str = "PRIVMSG #meow :hey dudes";
        let mut command: Command = str.parse().unwrap();
        assert_eq!(command.to_string(), str);

        str = "USER guest 0 * :Meow Tompski";
        command = str.parse().unwrap();
        assert_eq!(command.to_string(), str);

        str = "USER guest 0 * meow";
        command = str.parse().unwrap();
        assert_eq!(command.to_string(), str);

        str = "QUIT :Leaving";
        command = str.parse().unwrap();
        assert_eq!(command.to_string(), str);
    }
}
