use diesel::prelude::*;
use crate::schema::runs;

#[derive(Queryable)]
pub struct Run {
    pub id: i32,
    pub submitted: Option<String>,
    /// the thread we created for this run in discord
    pub thread_id: Option<String>,
    /// the run's id according to srdc
    pub run_id: String,
}

#[derive(Insertable)]
#[table_name="runs"]
pub struct NewRun<'a> {
    pub submitted: Option<&'a str>,
    pub thread_id: Option<String>,
    pub run_id: String,
}