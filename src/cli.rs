use clap::{Parser, Subcommand};
use url::Url;

#[derive(Debug, Clone, Parser)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<Command>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    Connect { url: Url },
    Disconnect,
    Kill,
    Keys,
    KeysWithPttl,
    Get { key: String },
    Set { key: String, value: String },
    Pexpire { key: String, ms: u64 },
    Exists { key: String },
    Pttl { key: String },
    Delete { key: String },
}
