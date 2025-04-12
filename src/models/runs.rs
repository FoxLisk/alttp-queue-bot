use crate::schema::runs;
use diesel::prelude::*;

#[derive(Queryable, Identifiable, Debug)]
pub struct Run {
    pub id: i32,
    /// a datetime string representing when the run was submitted
    pub submitted: Option<String>,
    /// the run's SRC id
    pub run_id: String,
}

#[derive(Identifiable, AsChangeset)]
#[table_name = "runs"]
// this is basically just because diesel hates enums
pub struct UpdateRun {
    id: i32,
    submitted: Option<String>,

    /// the run's id according to srdc
    run_id: String,
}

impl From<Run> for UpdateRun {
    fn from(r: Run) -> Self {
        UpdateRun {
            id: r.id,
            submitted: r.submitted,
            run_id: r.run_id,
        }
    }
}

#[derive(Insertable)]
#[table_name = "runs"]
pub struct NewRun<'a> {
    pub submitted: Option<&'a str>,
    #[diesel(serialize_as=String)]
    pub run_id: String,
}
