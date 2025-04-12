diesel::table! {
    category_aliases (id) {
        id -> Integer,
        game_src_id -> Text,
        category_src_id -> Text,
        alias -> Text,
    }
}

diesel::table! {
    runs (id) {
        id -> Integer,
        submitted -> Nullable<Text>,
        run_id -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(category_aliases, runs,);
