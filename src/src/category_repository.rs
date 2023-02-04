use crate::error::BotError;
use crate::models::aliases::CategoryAlias;
use crate::src::{get_categories, Category, SRCRun, Value};
use diesel::prelude::*;
use diesel::SqliteConnection;
use speedrun_api::api::categories::CategoryId;
use speedrun_api::api::games::GameId;
use speedrun_api::api::variables::{ValueId, VariableId};
use speedrun_api::SpeedrunApiClientAsync;
use std::collections::HashMap;

// i think this is kind of a bastardization of the ~*~Design Pattern~*~ Repository
pub struct CategoriesRepository<'a> {
    // game_id: GameId<'a>,
    categories: HashMap<CategoryId<'a>, Category<'a>>,
    /// rename categories from how they're displayed to something nicer
    aliases: HashMap<CategoryId<'a>, String>,
}

// i think we need a builder for this :\
impl<'a> CategoriesRepository<'a> {
    // should maybe be not pub. useful for testing without making actual web requests tho.
    pub fn new(categories: Vec<Category<'a>>, aliases: Vec<CategoryAlias>) -> Self {
        Self {
            categories: HashMap::from_iter(categories.into_iter().map(|c| (c.id.clone(), c))),
            aliases: HashMap::from_iter(
                aliases
                    .into_iter()
                    .map(|a| (a.category_src_id.into(), a.alias)),
            ),
        }
    }

    /// fetches data from the SRC API & local DB and creates a CategoriesRepository
    pub async fn new_with_fetch<'b, GID: Into<GameId<'a>>>(
        game_id: GID,
        src_client: &'b SpeedrunApiClientAsync,
        conn: &mut SqliteConnection,
    ) -> Result<CategoriesRepository<'b>, BotError> {
        let gid = game_id.into();
        let categories = get_categories(gid.clone(), src_client).await?;
        let aliases = CategoryAlias::by_game_id(&gid.to_string()).load(conn)?;
        Ok(CategoriesRepository::new(categories, aliases))
    }

    // kinda want a separate category type here
    // so i can just do `category.subcategory_name(...)` or whatever
    // using the data exactly as it comes in from the API is kind of annoying
    // this whole CategoriesRepository idea is to abstract over that, though; i could
    // certainly refactor this internally later
    fn subcategory_name<'b>(
        &self,
        category: &Category,
        values: &HashMap<VariableId<'b>, ValueId<'b>>,
    ) -> Option<String> {
        Self::subcategory(category, values).map(|(_vid, v)| v.label.clone())
    }

    fn subcategory<'b>(
        category: &'b Category,
        values: &'b HashMap<VariableId<'b>, ValueId<'b>>,
    ) -> Option<(&'b ValueId<'b>, &'b Value)> {
        for category_var in &category.variables.data {
            if category_var.is_subcategory {
                let subcat_id = values.get(&category_var.id)?;
                let subcat = category_var.values.values.get(subcat_id)?;
                return Some((subcat_id, subcat));
            }
        }
        None
    }

    // N.B. i don't fully understand why category has lifetime 'a here
    // i think it's because lifetimes are like... specifying the *maximum*, not the *actual*
    // lifetime? so as long as the output is 'a (doesn't outlive self), we can give
    // category a different lifetime (<'b: 'a>), but that basically just makes the function
    // signature more complicated
    fn _category_nice_name(&'a self, category: &'a Category<'_>) -> &'a String {
        self.aliases.get(&category.id).unwrap_or(&category.name)
    }

    pub fn category_name(&self, category_id: &CategoryId<'_>, values: &HashMap<VariableId<'a>, ValueId<'a>>) -> Option<String> {
        let cat = self.categories.get(category_id)?;
        let cat_name = self._category_nice_name(cat);
        match self.subcategory_name(cat, values) {
            // alttp uses "subcategories" kind of weirdly. our categories are rulesets and our
            // subcategories are categories. so the "category" is "No Major Glitches" and the
            // "subcategory" is "Any%"; this makes it read better to format it with the
            // subcategory first
            Some(sc) => Some(format!("{} {}", sc, cat_name)),
            None => Some(cat_name.clone()),
        }
    }

    pub fn category_name_from_run(&self, run: &SRCRun) -> Option<String> {
        self.category_name(&run.category, &run.values)
    }


}

#[cfg(test)]
mod tests {
    use crate::models::aliases::CategoryAlias;
    use crate::src::{CategoriesRepository, Category};

    #[test]
    fn test_nice_name() {
        let known_cat = Category {
            id: "asdf".into(),
            name: "my coool category".to_string(),
            variables: Default::default(),
        };
        let unknown_cat = Category {
            id: "lolz".into(),
            name: "oh noes".to_string(),
            variables: Default::default(),
        };

        let alias = CategoryAlias {
            id: 0,
            game_src_id: "irrelevant".to_string(),
            category_src_id: "asdf".to_string(),
            alias: "even cooler alias!".to_string(),
        };

        let cr = CategoriesRepository::new(vec![known_cat.clone()], vec![alias]);

        assert_eq!("even cooler alias!", cr._category_nice_name(&known_cat));
        assert_eq!("oh noes", cr._category_nice_name(&unknown_cat));
    }
}
