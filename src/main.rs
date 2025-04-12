extern crate diesel;
extern crate log4rs;
extern crate serde_json;
extern crate speedrun_api;

use diesel::prelude::*;
use diesel::SqliteConnection;
use diesel_migrations::MigrationHarness;
use rand::rng;
use rand::seq::IndexedRandom;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use twilight_model::channel::embed::Embed;
use twilight_model::channel::embed::EmbedField;

use alttp_queue_bot::discord_client::{BotDiscordClient, DiscordError};
use alttp_queue_bot::models::runs::{NewRun, Run};
use alttp_queue_bot::src::{get_runs, CategoriesRepository, SRCRun};
use alttp_queue_bot::utils::{env_var, format_hms, secs_to_millis};
use alttp_queue_bot::{error::*, get_conn, schema, ALTTP_GAME_ID};
use log::{debug, info, warn};
use speedrun_api::SpeedrunApiClientAsync;
use twilight_http::api_error::{ApiError, RatelimitedApiError};
use twilight_http::error::ErrorType;

/// mutates `db_run` in place
/// sleeps off discord time
async fn create_run_message(
    src_run: &SRCRun<'_>,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
) -> Result<(), BotError> {
    let titles = vec![
        "New PB arrived!",
        "Anotha one",
        "🚨 PB ALERT 🚨",
        "Meowski get on this one",
        "gaming?!",
        "someone was eating their wheaties",
        "get a load of this guy",
        "of all the runs I've seen...",
        "an ostentatious display of skill",
        "absolutely cracked",
        "this run is built different",
        "speed incarnate",
        "a true gamer moment",
        "unreal gaming skills",
        "this is peak performance",
        "legendary run",
        "a masterpiece of speedrunning",
        "this run deserves a medal",
        "phenomenal execution",
        "a run for the ages",
        "next-level gaming",
        "a true display of mastery",
        "this run is fire",
        "insane gameplay",
        "elite speedrunning",
        "this run is art 🎨",
        "unbelievable performance",
        "a run to remember",
        "probably spliced",
        "always check helma/arrghus!",
        "some people have all the luck!",
        "how many capespins in this one?",
        "doomtaDisdainfulDonny",
        "swifARTISTE",
        "will this one start a fight?",
        "is this WR pace?",
        "did someone say 'poggers'?",
        "this run is bussin' fr fr",
        "a certified hood classic",
        "MY GOAT",
        "absolutely no cap",
        "this run is sus 🕵️",
        "a true sigma grindset",
        "chef's kiss 👨‍🍳💋",
        "GoatEmotey",
        "superm209Eyes",
        "now THIS is a 24/7 Andy Watch Party",
        "its lmos league",
        "don't forget to show your keybinds!",
        "someone's hogging all the PB paste",
        "this needs to be retimed",
    ];

    let mut rng = rng();
    let title = titles.choose(&mut rng).map(|s| s.to_string());

    let embeds = vec![Embed {
        author: None,
        color: None,
        description: None,
        fields: vec![
            EmbedField {
                inline: true,
                name: "Runner".to_string(),
                value: src_run.player().unwrap_or("Unknown").to_string(),
            },
            EmbedField {
                inline: true,
                name: "Category".to_string(),
                value: categories
                    .category_name_from_run(src_run)
                    .unwrap_or("Unknown".to_string()),
            },
            EmbedField {
                inline: true,
                name: "Time".to_string(),
                value: format_hms(src_run.times.primary_t),
            },
        ],
        footer: None,
        image: None,
        kind: "rich".to_string(),
        provider: None,
        thumbnail: None,
        timestamp: None,
        title,
        url: Some(src_run.weblink.to_string()),
        video: None,
    }];

    let rli = discord_client
        .create_message(embeds)
        .await
        .map_err(BotError::from)?;
    rli.sleep().await;
    Ok(())
}

async fn handle_run(
    src_run: &SRCRun<'_>,
    runs_by_id: &mut HashMap<String, Run>,
    conn: &mut SqliteConnection,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
) -> Result<(), BotError> {
    let run_id = src_run.id.to_string();

    if let Some(_r) = runs_by_id.get(&run_id) {
        // this run is already in the db, so we don't need to do anything
        return Ok(());
    }

    create_run_message(&src_run, discord_client, categories).await?;
    // only create the run after we've posted about it, now that all we are doing is making
    // one post about it
    let new_run = NewRun {
        submitted: src_run.submitted.as_ref().map(|s| s.as_str()),
        run_id,
    };
    diesel::insert_into(schema::runs::table)
        .values(new_run)
        .execute(conn)?;

    Ok(())
}

/// scans SRC for new runs, creates records + threads for them
async fn handle_new_runs(
    src_client: &SpeedrunApiClientAsync,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
    conn: &mut SqliteConnection,
) -> Result<(), BotError> {
    // TODO(#4) this doesn't need to be a full table scan (and pulling this out to the caller might
    //          allow us to do fewer queries total, too)
    let known_runs = schema::runs::table.load::<Run>(conn)?;
    let mut runs_by_id: HashMap<String, Run> =
        HashMap::from_iter(known_runs.into_iter().map(|r| (r.run_id.clone(), r)));
    let runs = get_runs(&src_client).await?;
    info!("Processing {} runs in the src queue", runs.len());
    for run in runs {
        if let Err(e) = handle_run(&run, &mut runs_by_id, conn, discord_client, categories).await {
            if let BotError::DiscordError(DiscordError::HttpError(httpe)) = e {
                if let Some(rle) = http_error_to_ratelimit(httpe) {
                    // this is happening despite my efforts to avoid rate limits above, for some
                    // reason. best to just handle it i guess
                    let tts = secs_to_millis(rle.retry_after);
                    debug!(
                        "Rate limited processing {:?}: sleeping for {} -  {:?}",
                        run, tts, rle
                    );
                    tokio::time::sleep(Duration::from_millis(tts)).await;
                }
            } else {
                warn!("Error handling run {:?}: {:?}", run, e);
            }
        }
    }
    Ok(())
}

async fn run_once(
    src_client: &SpeedrunApiClientAsync,
    discord_client: &BotDiscordClient,
    categories: &CategoriesRepository<'_>,
    conn: &mut SqliteConnection,
) -> Result<(), BotError> {
    handle_new_runs(src_client, discord_client, categories, conn).await
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
    println!("Starting up");
    dotenv::dotenv().unwrap();
    let log_config_path = env_var("LOG4RS_CONFIG_FILE");
    log4rs::init_file(Path::new(&log_config_path), Default::default())
        .expect("Couldn't initialize logging");
    let src_client = SpeedrunApiClientAsync::new().unwrap();
    let discord_client = BotDiscordClient::new_from_env().unwrap();
    let database_url = env_var("DATABASE_URL");
    let mut diesel_conn = get_conn(&database_url).expect("Unable to connect to database");

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
            warn!("Error: {:?}", e);
        }
    }

    /*
    what could happen in the future:
        * automatic moderation based on discord actions
        * alttpce coverage
     */
}
