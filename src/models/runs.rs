use std::num::{NonZeroU64, ParseIntError};
use crate::schema::runs;
use diesel::prelude::*;
use diesel_enum_derive::DieselEnum;
use twilight_model::id::Id;
use twilight_model::id::marker::ChannelMarker;

#[derive(Debug, PartialEq, DieselEnum)]
pub enum RunState {
    None,
    ThreadCreated,
    MessageCreated,
    Finalized,
}


#[derive(Debug, PartialEq, DieselEnum)]
pub enum SRCState {
    New,
    Verified,
    Rejected,
    Unknown
}

impl SRCState {
    /// converts from the format SRC gives to an SRCState
    /// mostly this is just to handle capitalization, which I admit is clunky
    pub fn from_src_api_string(state: &str) -> Self {
        match state {
            "new" => Self::New,
            "verified" => Self::Verified,
            "rejected" => Self::Rejected,
            _ => Self::Unknown
        }
    }

    pub fn symbol(&self) -> char {
        match self {
            SRCState::New => {'🌱'}
            SRCState::Verified => {'☑'}
            SRCState::Rejected => {'❌'}
            SRCState::Unknown => {'❔'}
        }
    }
}

#[derive(Queryable, Identifiable, Debug)]
pub struct Run {
    pub id: i32,
    pub submitted: Option<String>,
    /// the thread we created for this run in discord
    pub thread_id: Option<String>,
    #[diesel(deserialize_as=String)]
    pub state: RunState,
    /// the run's SRC id
    pub run_id: String,
    /// the run's state on SRC
    #[diesel(deserialize_as=String)]
    pub src_state: SRCState,
}

#[derive(Debug)]
pub enum ThreadIdError {
    Missing,
    ParseIntError(ParseIntError)
}

impl From<ParseIntError> for ThreadIdError {
    fn from(pie: ParseIntError) -> Self {
        Self::ParseIntError(pie)
    }
}

impl Run {
    pub fn thread_id(&self) -> Result<Id<ChannelMarker>, ThreadIdError> {
        match &self.thread_id {
            Some(t) => {
                Ok(Id::<ChannelMarker>::from(t.parse::<NonZeroU64>()?))
            }
            None => {
                Err(ThreadIdError::Missing)
            }

        }
    }
}

#[derive(Identifiable, AsChangeset)]
#[table_name = "runs"]
// this is basically just because diesel hates enums
pub struct UpdateRun {
    id: i32,
    submitted: Option<String>,
    /// the thread we created for this run in discord
    thread_id: Option<String>,
    state: String,
    /// the run's id according to srdc
    run_id: String,
    src_state: String,
}

impl From<Run> for UpdateRun {
    fn from(r: Run) -> Self {
        UpdateRun {
            id: r.id,
            submitted: r.submitted,
            thread_id: r.thread_id,
            state: String::from(r.state),
            run_id: r.run_id,
            src_state: String::from(r.src_state),
        }
    }
}

#[derive(Insertable)]
#[table_name = "runs"]
pub struct NewRun<'a> {
    pub submitted: Option<&'a str>,
    pub thread_id: Option<String>,
    #[diesel(serialize_as=String)]
    pub state: RunState,
    pub run_id: String,
    #[diesel(serialize_as=String)]
    pub src_state: SRCState,
}
