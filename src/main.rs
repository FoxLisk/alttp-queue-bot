extern crate diesel;
extern crate serde_json;
extern crate speedrun_api;

use diesel::prelude::*;
use diesel::result::Error;
use diesel::{Connection, SqliteConnection};
use std::collections::{HashMap, HashSet};
use std::env;
use std::env::VarError;
use std::fmt::Debug;
use std::fs::read_to_string;
use std::num::{NonZeroU64, ParseIntError};
use std::time::Duration;

use crate::models::NewRun;
use speedrun_api::api::runs::RunId;
use speedrun_api::api::Root;
use speedrun_api::SpeedrunApiClientAsync;
use twilight_http::{Client, Response};
use twilight_http::response::DeserializeBodyError;
use twilight_model::channel::{Channel, ChannelType, Webhook};
use twilight_model::id::marker::{ApplicationMarker, ChannelMarker, WebhookMarker};
use twilight_model::id::Id;
use twilight_util::link::webhook::parse;

use crate::src::{get_runs, CategoriesRepository, Category, Run, SRCError};
use crate::utils::{env_var, format_hms};

mod models;
mod schema;
mod src;
mod utils;

const ALTTP_GAME_ID: &str = "9d3rr0dl";

#[derive(Clone, Debug)]
// this structure is because we *really* need webhooks with tokens here, to be able to execute them,
// but the API returns a nullable token, which the twilight API faithfully reproduces, and
// I want zero .unwrap() calls in steady state code
struct WebhookInfo {
    id: Id<WebhookMarker>,
    token: String,
}

struct BotDiscordClient {
    application_id: Id<ApplicationMarker>,
    channel_id: Id<ChannelMarker>,
    client: Client,
    webhook: WebhookInfo,
}

#[derive(Debug)]
enum BotError {
    VariableMissing(VarError),
    VariableParseError(ParseIntError),
    DatabaseError(Error),
    SRCError(SRCError),
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

fn debug_str<D: Debug>(d: D) -> String {
    format!("{:?}", d)
}

fn discard<T>(_t: T) -> () {
    ()
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

impl BotDiscordClient {
    fn new_from_env() -> Result<Self, BotError> {
        let token = env::var("BOT_TOKEN")?;
        let application_id =
            Id::<ApplicationMarker>::from(env::var("APPLICATION_ID")?.parse::<NonZeroU64>()?);
        let channel_id = Id::<ChannelMarker>::from(env::var("CHANNEL_ID")?.parse::<NonZeroU64>()?);
        let client = Client::new(token);
        let webhook = WebhookInfo {
            id: Id::<WebhookMarker>::from(env::var("WEBHOOK_ID")?.parse::<NonZeroU64>()?),
            token: env::var("WEBHOOK_TOKEN")?,
        };
        Ok(Self {
            client,
            application_id,
            channel_id,
            webhook,
        })
    }

    // TODO: this should return ratelimiting info (discovering the ratelimit by getting an error
    //       response isn't really ideal)
    async fn create_thread(
        &self,
        thread_name: &str,
        first_message_content: &str,
    ) -> Result<Channel, DiscordError> {
        let resp = self
            .client
            .create_thread(
                self.channel_id.clone(),
                thread_name,
                ChannelType::GuildPublicThread,
            ).map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await?;
        let channel = resp.model().await?;
        self.client
            .create_message(channel.id.clone())
            .content(first_message_content)
            .map_err(|e| DiscordError::ValidationError(e.to_string()))?
            .exec()
            .await
            .map(discard)?;
        Ok(channel)
    }

    // TODO: async fn validate_webhook or something like that
}

async fn get_webhook_by_url(client: &Client, url: String) -> Result<WebhookInfo, String> {
    let (id, tokeno) = parse(&url).map_err(|e| e.to_string())?;
    let token = tokeno.ok_or(format!("No token found for webhook {}", id))?;
    let resp: Response<Webhook> = match client.webhook(id).token(&token).exec().await {
        Ok(r) => r,
        Err(e) => {
            let er = format!("Error fetching webhook {}: {}", id, e);
            println!("{}", er);
            return Err(er);
        }
    };
    match resp.model().await {
        Ok(w) => Ok(WebhookInfo {
            id: w.id,
            token: w.token.ok_or("Webhook with no token".to_string())?,
        }),
        Err(e) => Err(e.to_string()),
    }
}

async fn run_once(
    src_client: &SpeedrunApiClientAsync,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
    conn: &mut SqliteConnection,
) -> Result<(), BotError> {
    let known_runs = schema::runs::table.load::<models::Run>(conn)?;
    let known_ids: HashSet<String> = HashSet::from_iter(known_runs.into_iter().map(|r| r.run_id));
    // let runs_with_embeds = read_to_string("api_responses/runs_embedded_players.json").unwrap();
    // let r = serde_json::from_str::<Root<Vec<Run>>>(&runs_with_embeds);
    let runs = get_runs(&src_client).await?;
    for run in runs {
        // it sucks that we have a str in here but we have to use to_string() which does a format
        // this is fine but it's against the _spirit_ of writing in rust to do unnecessary allocations!
        // maybe i could make the hashset have RunId<'a>s in it but that seems bad too kinda
        let run_id = run.id.to_string();
        if known_ids.contains(&run_id) {
            continue;
        }
        let thread_title = format!(
            "{} in {} by {}",
            categories
                .category_name(&run)
                .unwrap_or("Unknown category".to_string()),
            format_hms(run.times.primary_t),
            run.player().unwrap_or("Unknown player")
        );
        let mut new_run = NewRun {
            submitted: run.submitted.as_ref().map(|s| s.as_str()),
            thread_id: None,
            run_id,
        };
        match discord_client
            .create_thread(&thread_title, &run.weblink)
            .await
        {
            Ok(c) => new_run.thread_id = Some(c.id.to_string()),
            Err(e) => {
                // just let us try to catch this next time i guess?
                // we should probably create a record with null thread id... idk
                // maybe later
                if let DiscordError::HttpError(he) = e {
                    if let twilight_http::error::ErrorType::Response { error, .. } = he.kind() {
                        if let twilight_http::api_error::ApiError::Ratelimited(rl) = error {
                            println!("Rate limited on discord API: waiting {}", rl.retry_after);
                            tokio::time::sleep(Duration::from_secs(rl.retry_after.ceil() as u64)).await
                        }
                    }
                } else {
                    println!("Error creating thread: {:?}", e);
                }
                continue;
            }
        }
        // this should probably not fail early
        diesel::insert_into(schema::runs::table)
            .values(new_run)
            .execute(conn)?;
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().unwrap();
    let src_client = SpeedrunApiClientAsync::new().unwrap();
    let discord_client = Client::builder().build();

    let d_client = BotDiscordClient::new_from_env().unwrap();
    // let categories = get_categories(&src_client).await;
    let categories =
        read_to_string("api_responses/game_categories_embedded_variables.json").unwrap();
    let c = serde_json::from_str::<Root<Vec<Category>>>(&categories);
    let cr = CategoriesRepository::new(c.unwrap().data);

    let database_url = env_var("DATABASE_URL");
    let mut diesel_conn =
        SqliteConnection::establish(&database_url).expect("Unable to connect to database");
    let poll_interval = env_var("POLL_INTERVAL_SECS")
        .parse::<u64>()
        .expect("Unable to parse POLL_INTERVAL_SECS as an integer");
    let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
    loop {
        if let Err(e) = run_once(&src_client, &d_client, &cr, &mut diesel_conn).await {
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
