use diesel::prelude::*;
use crate::schema::category_aliases::dsl::*;

#[derive(Queryable)]
pub struct CategoryAlias {
    pub id: i32,
    pub game_src_id: String,
    pub category_src_id: String,
    pub alias: String,
}

impl CategoryAlias {
    pub fn by_game_id<'a>(game_id: &str, conn: &mut SqliteConnection) -> Result<Vec<Self>, diesel::result::Error> {
        category_aliases.filter(
            game_src_id.eq(game_id)
        ).load(conn)
    }
}