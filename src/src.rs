use crate::ALTTP_GAME_ID;
use futures_util::StreamExt;
use serde::Deserialize;
use speedrun_api::api;
use speedrun_api::api::categories::{CategoryEmbeds, CategoryId};
use speedrun_api::api::games::{GameCategories};
use speedrun_api::api::runs::{RunEmbeds, RunId, Runs};
use speedrun_api::types::{Names};
use speedrun_api::api::variables::{ValueId, VariableId};
use speedrun_api::api::{ApiError, AsyncQuery, PagedEndpointExt, Root};
use speedrun_api::error::RestError;
use speedrun_api::SpeedrunApiClientAsync;
use std::collections::HashMap;

pub type SRCError =  ApiError<RestError>;

#[derive(Deserialize, Debug)]
pub struct Value {
    pub label: String,
}

#[derive(Deserialize, Debug)]
pub struct Values<'a> {
    pub values: HashMap<ValueId<'a>, Value>,
}

#[derive(Deserialize, Debug)]
pub struct Variable<'a> {
    pub id: VariableId<'a>,
    pub category: Option<CategoryId<'a>>,
    #[serde(rename(deserialize="is-subcategory"))]
    pub is_subcategory: bool,
    pub values: Values<'a>,
}

#[derive(Deserialize, Debug)]
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
    // this really should never be null; it can only be null on very old runs
    // but i don't want to commit to it being non-null
    pub submitted: Option<String>,
    pub times: Times,
    pub values: HashMap<VariableId<'a>, ValueId<'a>>,
}

impl<'a> SRCRun<'a> {
    pub fn player(&self) -> Option<&str> {
        self.players.data.first().map(|p| p.name())
    }
}

#[derive(Deserialize, Debug)]
#[serde(tag = "rel")]
#[serde(rename_all="lowercase")]
pub enum PlayerEmbed {
    User {
        names: Names
    },
    Guest {
        name: String
    }
}

impl PlayerEmbed {
    pub fn name(&self) -> &str {
        match self {
            Self::User { names } => {
                &names.international
            }
            Self::Guest { name } => {
                &name
            }
        }
    }
}

// i think this is kind of a bastardization of the ~*~Design Pattern~*~ Repository
pub struct CategoriesRepository<'a> {
    // game_id: GameId<'a>,
    categories: HashMap<CategoryId<'a>, Category<'a>>
}

impl<'a> CategoriesRepository<'a> {
    // should maybe be not pub. useful for testing without making actual web requests tho.
    pub fn new(categories: Vec<Category<'a>>) -> Self {
        Self {
            categories: HashMap::from_iter(
                categories.into_iter().map(|c| (c.id.clone(), c))
            )
        }
    }

    /// fetches data from the SRC API and creates a CategoriesRepository
    pub async fn new_with_fetch(src_client: &SpeedrunApiClientAsync,) -> Result<CategoriesRepository<'_>, SRCError> {
        let categories = get_categories(src_client)
            .await?;
        Ok(CategoriesRepository::new(categories))
    }

    // kinda want a separate category type here
    // so i can just do `category.subcategory_name(...)` or whatever
    // using the data exactly as it comes in from the API is kind of annoying
    // this whole CategoriesRepository idea is to abstract over that, though; i could
    // certainly refactor this internally later
    fn subcategory_name<'b>(category: &Category, values: &HashMap<VariableId<'b>, ValueId<'b>>) -> Option<String> {
        for var in &category.variables.data {
            if var.is_subcategory {
                let subcat_id = values.get(&var.id)?;
                let subcat_name = var.values.values.get(subcat_id)?;
                return Some(subcat_name.label.clone());
            }
        }
        None
    }

    pub fn category_name(&self, run: &SRCRun) -> Option<String> {
        let cat = self.categories.get(&run.category)?;
        match Self::subcategory_name(cat, &run.values) {
            // alttp uses "subcategories" kind of weirdly. our categories are rulesets and our
            // subcategories are categories. so the "category" is "No Major Glitches" and the
            // "subcategory" is "Any%"; this makes it read better to format it with the
            // subcategory first
            Some(sc) => Some(format!("{} {}", sc, cat.name)),
            None => Some(cat.name.clone())
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
        .build()
        .unwrap();

    let mut runs_stream = runs.stream::<SRCRun, SpeedrunApiClientAsync>(&src_client);

    let mut runs = vec![];
    while let Some(t) = runs_stream.next().await {
        match t {
            Ok(r) => {runs.push(r)},
            Err(e) => {
                println!("Error fetching run: {:?}", e);
                continue;
            }
        };
    }
    Ok(runs)
}

pub async fn get_categories(
    src_client: &SpeedrunApiClientAsync,
) -> Result<Vec<Category<'_>>, SRCError> {
    // we're gonna just get category-relevant variables in here because i don't care about
    // blue balls
    let categories_q: GameCategories = GameCategories::builder()
        .id(ALTTP_GAME_ID)
        .miscellaneous(false)
        .embed(CategoryEmbeds::Variables)
        .build()
        .unwrap();
    categories_q.query_async(src_client).await
}

