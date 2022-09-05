// @generated automatically by Diesel CLI.

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
        thread_id -> Nullable<Text>,
        state -> Text,
        run_id -> Text,
        src_state -> Text,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    category_aliases,
    runs,
);
