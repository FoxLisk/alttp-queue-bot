#[macro_use]
extern crate diesel;
extern crate diesel_enum_derive;
extern crate serde_json;
extern crate speedrun_api;

use diesel::prelude::*;
use diesel::{Connection, SqliteConnection};
use diesel_migrations::MigrationHarness;
use std::collections::HashMap;
use std::num::NonZeroU64;
use std::time::Duration;

use alttp_queue_bot::discord_client::{BotDiscordClient, DiscordError, RateLimitInfo};
use alttp_queue_bot::models::runs::RunState::ThreadCreated;
use alttp_queue_bot::models::runs::{NewRun, Run, RunState, UpdateRun};
use alttp_queue_bot::src::{get_runs, CategoriesRepository, SRCRun};
use alttp_queue_bot::utils::{env_var, format_hms, secs_to_millis};
use alttp_queue_bot::{error::*, schema, ALTTP_GAME_ID};
use speedrun_api::SpeedrunApiClientAsync;
use twilight_http::api_error::{ApiError, RatelimitedApiError};
use twilight_http::error::ErrorType;
use twilight_model::id::marker::ChannelMarker;
use twilight_model::id::Id;

/// mutates `db_run` in place
async fn create_run_thread(
    src_run: &SRCRun<'_>,
    db_run: &mut Run,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
) -> Result<Option<RateLimitInfo>, DiscordError> {
    if RunState::None != db_run.state {
        return Ok(None);
    }
    let thread_title = format!(
        "{} - {} in {}",
        src_run.player().unwrap_or("Unknown player"),
        categories
            .category_name(src_run)
            .unwrap_or("Unknown category".to_string()),
        format_hms(src_run.times.primary_t),
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
        return Ok(None);
    }
    let thread_id = match &db_run.thread_id {
        Some(t) => t,
        None => {
            return Err(BotError::InvalidState(format!(
                "Run {} was in state {} but has no thread id",
                db_run.id,
                String::from(&db_run.state),
            )));
        }
    };
    let channel_id = Id::<ChannelMarker>::from(thread_id.parse::<NonZeroU64>()?);
    discord_client
        .create_message(channel_id, &src_run.weblink)
        .await
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
    if let Some(second_rli) = create_run_message(&src_run, &mut run, discord_client).await? {
        if second_rli.remaining == 0 {
            println!("About to be rate limited on create message: sleeping it off...");
            tokio::time::sleep(Duration::from_millis(second_rli.reset_after_millis())).await;
        }
    }

    let changes = UpdateRun::from(run);
    diesel::update(&changes).set(&changes).execute(conn)?;
    Ok(())
}

async fn run_once(
    src_client: &SpeedrunApiClientAsync,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
    conn: &mut SqliteConnection,
) -> Result<(), BotError> {
    let known_runs = schema::runs::table.load::<Run>(conn)?;
    let mut runs_by_id: HashMap<String, Run> =
        HashMap::from_iter(known_runs.into_iter().map(|r| (r.run_id.clone(), r)));
    let runs = get_runs(&src_client).await?;
    println!("Processing {} runs in the src queue", runs.len());
    for run in runs {
        if let Err(e) = handle_run(&run, &mut runs_by_id, conn, discord_client, categories).await {
            if let BotError::DiscordError(DiscordError::HttpError(httpe)) = e {
                if let Some(rle) = http_error_to_ratelimit(httpe) {
                    // this is happening despite my efforts to avoid rate limits above, for some
                    // reason. best to just handle it i guess
                    let tts = secs_to_millis(rle.retry_after);
                    println!(
                        "Rate limited processing {:?}: sleeping for {} -  {:?}",
                        run, tts, rle
                    );
                    tokio::time::sleep(Duration::from_millis(tts)).await;
                }
            } else {
                println!("Error handling run {:?}: {:?}", run, e);
            }
        }
    }
    Ok(())
}

fn http_error_to_ratelimit(httpe: twilight_http::Error) -> Option<RatelimitedApiError> {
    let (kind, _) = httpe.into_parts();
    match kind {
        ErrorType::Response { error, .. } => match error {
            ApiError::Ratelimited(rl) => Some(rl),
            _ => None,
        },
        _ => None,
    }
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().unwrap();
    let src_client = SpeedrunApiClientAsync::new().unwrap();
    let discord_client = BotDiscordClient::new_from_env().unwrap();
    let database_url = env_var("DATABASE_URL");
    let mut diesel_conn =
        SqliteConnection::establish(&database_url).expect("Unable to connect to database");

    let migrations = diesel_migrations::FileBasedMigrations::find_migrations_directory().unwrap();
    diesel_conn.run_pending_migrations(migrations).unwrap();

    let cr = CategoriesRepository::new_with_fetch(ALTTP_GAME_ID, &src_client, &mut diesel_conn)
        .await
        .unwrap();

    let poll_interval = env_var("POLL_INTERVAL_SECS")
        .parse::<u64>()
        .expect("Unable to parse POLL_INTERVAL_SECS as an integer");
    let mut interval = tokio::time::interval(Duration::from_secs(poll_interval));
    loop {
        interval.tick().await;
        if let Err(e) = run_once(&src_client, &discord_client, &cr, &mut diesel_conn).await {
            println!("Error: {:?}", e);
        }
    }

    /*
    what could happen in the future:
        * automatic moderation based on discord actions
        * alttpce coverage
     */
}
