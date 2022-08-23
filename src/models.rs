use diesel::prelude::*;
// use diesel::query_builder::AsChangeset;
use diesel_enum_derive::DieselEnum;
use crate::schema::runs;

#[derive(Debug, PartialEq, DieselEnum)]
pub enum RunState {
    None,
    ThreadCreated,
    MessageCreated,
}


#[derive(Queryable, Identifiable)]
pub struct Run {
    pub id: i32,
    pub submitted: Option<String>,
    /// the thread we created for this run in discord
    pub thread_id: Option<String>,
    #[diesel(deserialize_as=String)]
    pub state: RunState,
    /// the run's id according to srdc
    pub run_id: String,
}


#[derive(Identifiable, AsChangeset)]
#[table_name="runs"]
// this is basically just because diesel hates enums
pub struct UpdateRun {
    id: i32,
    submitted: Option<String>,
    /// the thread we created for this run in discord
    thread_id: Option<String>,
    state: String,
    /// the run's id according to srdc
    run_id: String,
}

impl From<Run> for UpdateRun {
    fn from(r: Run) -> Self {
        UpdateRun {
            id: r.id,
            submitted: r.submitted,
            thread_id: r.thread_id,
            state: String::from(r.state),
            run_id: r.run_id,
        }
    }
}


#[derive(Insertable)]
#[table_name="runs"]
pub struct NewRun<'a> {
    pub submitted: Option<&'a str>,
    pub thread_id: Option<String>,
    #[diesel(serialize_as=String)]
    pub state: RunState,
    pub run_id: String,
}