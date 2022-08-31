use crate::discord_client::DiscordError;
use crate::src::SRCError;
use diesel::result::Error;
use std::env::VarError;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum BotError {
    VariableMissing(VarError),
    VariableParseError(ParseIntError),
    DatabaseError(Error),
    SRCError(SRCError),
    DiscordError(DiscordError),
    InvalidState(String),
}

impl From<VarError> for BotError {
    fn from(ve: VarError) -> Self {
        Self::VariableMissing(ve)
    }
}

impl From<ParseIntError> for BotError {
    fn from(pie: ParseIntError) -> Self {
        Self::VariableParseError(pie)
    }
}

impl From<Error> for BotError {
    fn from(e: Error) -> Self {
        Self::DatabaseError(e)
    }
}

impl From<SRCError> for BotError {
    fn from(e: SRCError) -> Self {
        Self::SRCError(e)
    }
}

impl From<DiscordError> for BotError {
    fn from(e: DiscordError) -> Self {
        Self::DiscordError(e)
    }
}
