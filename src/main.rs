extern crate diesel;
extern crate serde_json;
extern crate speedrun_api;
extern crate diesel_enum_derive;

use diesel::prelude::*;
use diesel::result::Error;
use diesel::{Connection, SqliteConnection};
use std::collections::{HashMap};
use std::env;
use std::env::VarError;
use std::fmt::Debug;
use std::num::{NonZeroU64, ParseIntError};
use std::time::Duration;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};

use crate::models::{NewRun, Run, RunState, UpdateRun};
use speedrun_api::SpeedrunApiClientAsync;
use twilight_http::{Client};
use twilight_http::api_error::{ApiError, RatelimitedApiError};
use twilight_http::error::ErrorType;
use twilight_http::response::{DeserializeBodyError, HeaderIter};
use twilight_model::channel::{Channel, ChannelType};
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker};
use twilight_model::id::Id;
use crate::models::RunState::ThreadCreated;

use crate::src::{get_runs, CategoriesRepository, SRCError, SRCRun};
use crate::utils::{env_var, format_hms, secs_to_millis};

mod models;
mod schema;
mod src;
mod utils;

const ALTTP_GAME_ID: &str = "9d3rr0dl";

struct BotDiscordClient {
    application_id: Id<ApplicationMarker>,
    channel_id: Id<ChannelMarker>,
    client: Client,
}

#[derive(Debug)]
enum BotError {
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


#[derive(Debug)]
enum DiscordError {
    HttpError(twilight_http::Error),
    ValidationError(String),
    DeserializeBodyError(DeserializeBodyError)
}

impl From<twilight_http::Error> for DiscordError {
    fn from(e: twilight_http::Error) -> Self {
        Self::HttpError(e)
    }
}

impl From<DeserializeBodyError> for DiscordError {
    fn from(e: DeserializeBodyError) -> Self {
        Self::DeserializeBodyError(e)
    }
}

#[derive(Debug)]
struct RateLimitInfo {
    bucket: String,
    reset_after: f64,
    remaining: u64,
}

impl RateLimitInfo {
    fn from_headers(headers: HeaderIter) -> Option<Self> {
        let mut builder = RateLimitInfoBuilder::new();
        for (name, val) in headers {
            match name {
                "x-ratelimit-remaining" => {builder.remaining(val);},
                "x-ratelimit-reset-after" => {builder.reset_after(val);},
                "x-ratelimit-bucket" => {builder.bucket(val);}
                _ => {}
            }
        }
        builder.build()
    }

    fn reset_after_millis(&self) -> u64 {
        secs_to_millis(self.reset_after)
    }
}

struct RateLimitInfoBuilder<'b> {
    bucket: Option<&'b str>,
    reset_after: Option<f64>,
    remaining: Option<u64>,
}
impl<'b> RateLimitInfoBuilder<'b> {
    fn new() -> Self {
        Self {
            bucket: None,
            reset_after: None,
            remaining: None,
        }
    }
    fn bucket(&mut self, header_value: &'b [u8]) {
        if let Ok(v) = std::str::from_utf8(header_value) {
            self.bucket = Some(v);
        }
    }

    fn reset_after(&mut self, header_value: &'b [u8]) {
        if let Ok(s) = std::str::from_utf8(header_value) {
            if let Ok(v) = s.parse() {
                self.reset_after = Some(v);
            }
        }
    }
    fn remaining(&mut self, header_value: &'b [u8]) {
        if let Ok(s) = std::str::from_utf8(header_value) {
            if let Ok(v) = s.parse() {
                self.remaining = Some(v);
            }
        }
    }
    fn build(self) -> Option<RateLimitInfo> {
        match (self.reset_after, self.remaining, self.bucket) {
            (Some(reset_after), Some(remaining), Some(bucket)) => {
                Some(RateLimitInfo {
                    reset_after,
                    remaining,
                    bucket: bucket.to_string()
                })
            },
            _ => None
        }
    }
}


impl BotDiscordClient {
    fn new_from_env() -> Result<Self, BotError> {
        let token = env::var("BOT_TOKEN")?;
        let application_id =
            Id::<ApplicationMarker>::from(env::var("APPLICATION_ID")?.parse::<NonZeroU64>()?);
        let channel_id = Id::<ChannelMarker>::from(env::var("CHANNEL_ID")?.parse::<NonZeroU64>()?);
        let client = Client::new(token);
        Ok(Self {
            client,
            application_id,
            channel_id,
        })
    }

    // TODO: this should return ratelimiting info (discovering the ratelimit by getting an error
    //       response isn't really ideal)
    async fn create_thread(
        &self,
        thread_name: &str,
    ) -> Result<(Option<RateLimitInfo>, Channel), DiscordError> {
        let resp = self
            .client
            .create_thread(
                self.channel_id.clone(),
                thread_name,
                ChannelType::GuildPublicThread,
            ).map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await?;
        let rli = RateLimitInfo::from_headers(resp.headers());
        let channel = resp.model().await?;
        Ok((rli, channel))
    }

    async fn create_message(
        &self,
        channel: Id<ChannelMarker>,
        content: &str,
    ) -> Result<Option<RateLimitInfo>, DiscordError> {
        let resp = self.client
            .create_message(channel)
            .content(content)
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await?;
        Ok(RateLimitInfo::from_headers(resp.headers()))
    }
    // TODO: async fn validate_webhook or something like that
}


/// mutates `db_run` in place
async fn create_run_thread(
    src_run: &SRCRun<'_>,
    db_run: &mut Run,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>
) -> Result<Option<RateLimitInfo>, DiscordError> {
    if RunState::None != db_run.state {
        return Ok(None)
    }
    let thread_title = format!(
        "{} in {} by {}",
        categories
            .category_name(src_run)
            .unwrap_or("Unknown category".to_string()),
        format_hms(src_run.times.primary_t),
        src_run.player().unwrap_or("Unknown player")
    );

    discord_client
        .create_thread(&thread_title)
        .await
        .map(|(rli, c)| {
            db_run.thread_id = Some(c.id.to_string());
            db_run.state = ThreadCreated;
            rli
    })
}

/// mutates `db_run` in place
async fn create_run_message(
    src_run: &SRCRun<'_>,
    db_run: &mut Run,
    discord_client: &BotDiscordClient,
) -> Result<Option<RateLimitInfo>, BotError> {
    if ThreadCreated != db_run.state {
        return Ok(None)
    }
    let thread_id = match &db_run.thread_id {
        Some(t) => t,
        None => {
            return Err(BotError::InvalidState(
                format!("Run {} was in state {} but has no thread id",
                    db_run.id,
                    String::from(&db_run.state),

            )));
        }
    };
    let channel_id = Id::<ChannelMarker>::from(thread_id.parse::<NonZeroU64>()?);
    discord_client.create_message(
        channel_id,
        &src_run.weblink
    ).await
        .map_err(BotError::from)
        .map(|c| {
          db_run.state = RunState::MessageCreated;
            c
        })
}

async fn handle_run(
    src_run: &SRCRun<'_>,
    runs_by_id: &mut HashMap<String, Run>,
    conn: &mut SqliteConnection,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
) -> Result<(), BotError> {
    let run_id = src_run.id.to_string();
    let mut run = match runs_by_id.remove(&run_id) {
        Some(r) => r,
        None => {
            let new_run = NewRun {
                submitted: src_run.submitted.as_ref().map(|s| s.as_str()),
                state: RunState::None,
                thread_id: None,
                run_id,
            };
            diesel::insert_into(schema::runs::table)
                .values(new_run)
                .get_result(conn)?
        }
    };

    if let Some(rli) = create_run_thread(&src_run, &mut run, discord_client, categories).await? {
        if rli.remaining == 0 {
            println!("About to be rate limited on create thread: sleeping it off...");
            tokio::time::sleep(Duration::from_millis(rli.reset_after_millis())).await;
        }
    }
    if let Some(second_rli) = create_run_message(
        &src_run, &mut run, discord_client
    ).await? {
        if second_rli.remaining == 0 {
            println!("About to be rate limited on create message: sleeping it off...");
            tokio::time::sleep(Duration::from_millis(second_rli.reset_after_millis())).await;
        }
    }

    let changes = UpdateRun::from(run);
    diesel::update(&changes).set(&changes)
        .execute(conn)?;
    Ok(())
}

async fn run_once(
    src_client: &SpeedrunApiClientAsync,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
    conn: &mut SqliteConnection,
) -> Result<(), BotError> {
    let known_runs = schema::runs::table.load::<Run>(conn)?;
    let mut runs_by_id: HashMap<String, Run> = HashMap::from_iter(known_runs.into_iter().map(|r| (r.run_id.clone(), r)));
    // let runs_with_embeds = read_to_string("api_responses/runs_embedded_players.json").unwrap();
    // let r = serde_json::from_str::<Root<Vec<Run>>>(&runs_with_embeds);
    let runs = get_runs(&src_client).await?;
    println!("Processing {} runs in the src queue", runs.len());
    for run in runs {
        // it sucks that we have a str in here but we have to use to_string() which does a format
        // this is fine but it's against the _spirit_ of writing in rust to do unnecessary allocations!
        // maybe i could make the hashset have RunId<'a>s in it but that seems bad too kinda
        if let Err(e) = handle_run(
            &run, &mut runs_by_id, conn, discord_client, categories
        ).await {
            if let BotError::DiscordError(DiscordError::HttpError(httpe)) = e {
                if let Some(rle) = http_error_to_ratelimit(httpe) {
                    // this is happening despite my efforts to avoid rate limits above, for some
                    // reason. best to just handle it i guess
                    let tts = secs_to_millis(rle.retry_after);
                    println!("Rate limited processing {:?}: sleeping for {} -  {:?}", run, tts, rle);
                    tokio::time::sleep(Duration::from_millis(tts)).await;
                }
            }else {
                println!("Error handling run {:?}: {:?}", run, e);
            }
        }
    }
    Ok(())
}

fn http_error_to_ratelimit(httpe: twilight_http::Error) -> Option<RatelimitedApiError> {
    let (kind, _) = httpe.into_parts();
    match kind {
        ErrorType::Response { error, ..  } => {
            match error {
                ApiError::Ratelimited(rl) => {
                    Some(rl)
                }
                _ => None
            }
        }
        _ => None
    }
}

const MIGRATIONS: EmbeddedMigrations = diesel_migrations::embed_migrations!();

#[tokio::main]
async fn main() {
    dotenv::dotenv().unwrap();
    let src_client = SpeedrunApiClientAsync::new().unwrap();
    let discord_client = BotDiscordClient::new_from_env().unwrap();
    // let categories =
    //     read_to_string("api_responses/game_categories_embedded_variables.json").unwrap();
    // let c = serde_json::from_str::<Root<Vec<Category>>>(&categories);
    let cr = CategoriesRepository::new_with_fetch(&src_client).await.unwrap();

    let database_url = env_var("DATABASE_URL");
    let mut diesel_conn =
        SqliteConnection::establish(&database_url).expect("Unable to connect to database");

    diesel_conn.run_pending_migrations(MIGRATIONS).unwrap();

    let poll_interval = env_var("POLL_INTERVAL_SECS")
        .parse::<u64>()
        .expect("Unable to parse POLL_INTERVAL_SECS as an integer");
    let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
    loop {
        if let Err(e) = run_once(&src_client, &discord_client, &cr, &mut diesel_conn).await {
            println!("Error: {:?}", e);
        }
        interval.tick().await;
    }
    /*
    what could happen in the future:
        * automatic moderation based on discord actions
        * alttpce coverage
     */
}
