#[macro_use]
extern crate diesel;

pub mod discord_client;
pub mod error;
pub mod models;
pub mod schema;
pub mod src;
pub mod utils;

pub const ALTTP_GAME_ID: &str = "9d3rr0dl";
