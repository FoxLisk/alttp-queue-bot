use diesel::helper_types::{Eq, Filter};
use diesel::internal::table_macro::FromClause;
use diesel::prelude::*;
use crate::ALTTP_GAME_ID;
use crate::schema::category_aliases::dsl::*;

#[derive(Queryable)]
pub struct CategoryAlias {
    pub id: i32,
    pub game_src_id: String,
    pub category_src_id: String,
    pub alias: String,
}


impl CategoryAlias {
    pub fn by_game_id(game_id: &str) -> Filter<category_aliases, Eq<game_src_id, &str>> {
        category_aliases.filter(
            game_src_id.eq(game_id)
        )
    }
}