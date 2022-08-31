table! {
    category_aliases (id) {
        id -> Integer,
        game_src_id -> Text,
        category_src_id -> Text,
        alias -> Text,
    }
}

table! {
    runs (id) {
        id -> Integer,
        submitted -> Nullable<Text>,
        thread_id -> Nullable<Text>,
        state -> Text,
        run_id -> Text,
    }
}

allow_tables_to_appear_in_same_query!(category_aliases, runs,);
