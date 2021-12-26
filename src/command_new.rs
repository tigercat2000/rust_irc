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
type Unused = String;
type NicknameMask = String;

enum Command {    
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
}

struct Message {
    tags: Option<Vec<String>>,
    source: Option<String>,
    command: Command
}