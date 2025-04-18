mod category_repository;

use crate::ALTTP_GAME_ID;

use futures_util::StreamExt;
use serde::Deserialize;
use speedrun_api::api;
use speedrun_api::api::categories::{CategoryEmbeds, CategoryId};
use speedrun_api::api::games::{GameCategories, GameCategoriesBuilderError, GameId};
use speedrun_api::api::runs::{Run, RunBuilderError, RunEmbeds, RunId, Runs, RunsBuilderError};
use speedrun_api::api::variables::{ValueId, VariableId};
use speedrun_api::api::{ApiError, AsyncQuery, PagedEndpointExt, Root};
use speedrun_api::error::RestError;
use speedrun_api::types::{Names, Status};
use speedrun_api::SpeedrunApiClientAsync;
use std::collections::HashMap;

// pub type SRCError = ApiError<RestError>;

#[derive(Debug)]
pub enum SRCError {
    /// error getting response from SRC API
    ApiError(ApiError<RestError>),
    /// error constructing the query
    /// (stringified because I don't care that much about every variant)
    QueryBuildError(String),
}

impl From<ApiError<RestError>> for SRCError {
    fn from(apie: ApiError<RestError>) -> Self {
        Self::ApiError(apie)
    }
}

impl From<RunBuilderError> for SRCError {
    fn from(rbe: RunBuilderError) -> Self {
        Self::QueryBuildError(rbe.to_string())
    }
}


impl From<RunsBuilderError> for SRCError {
    fn from(rbe: RunsBuilderError) -> Self {
        Self::QueryBuildError(rbe.to_string())
    }
}

impl From<GameCategoriesBuilderError> for SRCError {
    fn from(gcbe: GameCategoriesBuilderError) -> Self {
        Self::QueryBuildError(gcbe.to_string())
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Value {
    pub label: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Values<'a> {
    pub values: HashMap<ValueId<'a>, Value>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Variable<'a> {
    pub id: VariableId<'a>,
    pub category: Option<CategoryId<'a>>,
    #[serde(rename(deserialize = "is-subcategory"))]
    pub is_subcategory: bool,
    pub values: Values<'a>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Category<'a> {
    pub id: CategoryId<'a>,
    pub name: String,
    pub variables: Root<Vec<Variable<'a>>>,
}

#[derive(Deserialize, Debug)]
// N.B. I am deliberately ignoring all the other crap in here because we don't need it,
// and if we ever need it, that should be a conscious change
pub struct Times {
    pub primary: String,
    // this should always be an integer for ALttP
    pub primary_t: f64,
}

#[derive(Deserialize, Debug)]
pub struct SRCRun<'a> {
    pub id: RunId<'a>,
    pub weblink: String,
    pub category: CategoryId<'a>,
    pub players: Root<Vec<PlayerEmbed>>,
    pub status: Option<Status<'a>>,
    // this is a datetime of when the run was submitted
    // it really should never be null; it can only be null on very old runs
    // but i don't want to commit to it being non-null
    pub submitted: Option<String>,
    pub times: Times,
    pub values: HashMap<VariableId<'a>, ValueId<'a>>,
}

#[derive(Deserialize, Debug)]
pub struct SRCRunStatus<'a> {
    pub id: RunId<'a>,
    pub status: Status<'a>,
}

impl<'a> SRCRun<'a> {
    pub fn player(&self) -> Option<&str> {
        self.players.data.first().map(|p| p.name())
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "rel")]
#[serde(rename_all = "lowercase")]
pub enum PlayerEmbed {
    User { names: Names },
    Guest { name: String },
}

impl PlayerEmbed {
    pub fn name(&self) -> &str {
        match self {
            Self::User { names } => &names.international,
            Self::Guest { name } => &name,
        }
    }
}

pub async fn get_runs(src_client: &SpeedrunApiClientAsync) -> Result<Vec<SRCRun<'_>>, SRCError> {
    let runs: Runs = Runs::builder()
        .status(api::runs::RunStatus::New)
        .game(ALTTP_GAME_ID)
        .orderby(api::runs::RunsSorting::Submitted)
        .direction(api::Direction::Asc)
        .embed(RunEmbeds::Players)
        .build()?;

    let mut runs_stream = runs.stream::<SRCRun, SpeedrunApiClientAsync>(&src_client);

    let mut runs = vec![];
    while let Some(t) = runs_stream.next().await {
        match t {
            Ok(r) => runs.push(r),
            Err(e) => {
                println!("Error fetching run: {:?}", e);
                continue;
            }
        };
    }
    Ok(runs)
}

pub async fn get_run<'a, T: Into<RunId<'a>>>(src_client: &SpeedrunApiClientAsync, id: T) -> Result<SRCRunStatus<'a>, SRCError> {
    let run_id = id.into();
    let gr = Run::builder().id(run_id).build()?;
    let o = gr.query_async(src_client).await;
    o.map_err(Into::into)
}

pub async fn get_categories<'a, GID: Into<GameId<'a>>>(
    game_id: GID,
    src_client: &SpeedrunApiClientAsync,
) -> Result<Vec<Category<'_>>, SRCError> {
    // we're gonna just get category-relevant variables in here because i don't care about
    // blue balls
    let categories_q: GameCategories = GameCategories::builder()
        .id(game_id)
        .miscellaneous(false)
        .embed(CategoryEmbeds::Variables)
        .build()?;
    categories_q.query_async(src_client).await.map_err(Into::into)
}

pub use category_repository::CategoriesRepository;
