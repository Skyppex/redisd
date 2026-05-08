use std::str::FromStr;

use clap::{Parser, Subcommand};
use url::Url;

#[derive(Debug, Clone, Parser)]
pub struct Cli {
    #[arg(short = 't', long)]
    pub idle_timeout: Option<Duration>,

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

#[derive(Debug, Clone)]
pub struct Duration {
    pub milliseconds: u64,
}

impl FromStr for Duration {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let split_at = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let (num_str, unit) = s.split_at(split_at);

        let num = num_str
            .parse::<u64>()
            .map_err(|_| format!("invalid number: {num_str:?}"))?;

        let milliseconds = match unit.trim() {
            "ms" | "" => num,
            "s" | "sec" => num * 1000,
            "m" | "min" => num * 60 * 1000,
            "h" | "hr" | "hour" => num * 60 * 60 * 1000,
            other => return Err(format!("unknown unit: {other:?}")),
        };

        Ok(Duration { milliseconds })
    }
}
