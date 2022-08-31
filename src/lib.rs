#[macro_use]
extern crate diesel;

pub mod discord_client;
pub mod utils;
pub mod src;
pub mod models;
pub mod schema;
pub mod error;

pub const ALTTP_GAME_ID: &str = "9d3rr0dl";

