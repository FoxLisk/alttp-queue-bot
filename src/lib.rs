extern crate diesel;

use diesel::{Connection, ConnectionResult, SqliteConnection};

pub mod discord_client;
pub mod error;
pub mod models;
pub mod schema;
pub mod src;
pub mod utils;

pub const ALTTP_GAME_ID: &str = "9d3rr0dl";

pub fn get_conn(database_url: &str) -> ConnectionResult<SqliteConnection>{
    SqliteConnection::establish(&database_url)
}