use diesel::table;

table! {
    runs (id) {
        id -> Integer,
        submitted -> Nullable<Text>,
        thread_id -> Nullable<Text>,
        state -> Text,
        run_id -> Text,
    }
}
