use diesel::RunQueryDsl;
use diesel_migrations::{FileBasedMigrations, MigrationHarness};
use alttp_queue_bot::get_conn;
use alttp_queue_bot::models::runs::{NewRun, Run, RunState, SRCState, UpdateRun};
use alttp_queue_bot::schema::runs::dsl::id;
use alttp_queue_bot::schema::runs;

fn main() {
    let whatever = FileBasedMigrations::find_migrations_directory().unwrap();
    let mut db = get_conn(":memory:").unwrap();
    db.run_pending_migrations(whatever).unwrap();

    let sstate = SRCState::from_src_api_string("new");
    println!("{:?}", sstate);
    let nr = NewRun {
        submitted: None,
        thread_id: None,
        state: RunState::None,
        run_id: "".to_string(),
        src_state: sstate
    };

    diesel::insert_into(runs::table)
        .values(nr)
        .execute(&mut db)
        .unwrap();

    let mut r: Vec<Run> = runs::table.load::<Run>(&mut db).unwrap();
    let mut the_run = r.pop().unwrap();
    the_run.src_state = SRCState::Rejected;
    let update = UpdateRun::from(the_run);
    diesel::update(&update).set(&update).execute(&mut db).unwrap();

    let mut r: Vec<Run> = runs::table.load::<Run>(&mut db).unwrap();
    println!("{:?}", r);
}