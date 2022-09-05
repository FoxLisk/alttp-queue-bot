extern crate diesel_migrations;

use diesel::migration::{Migration, MigrationSource};
use diesel::sqlite::Sqlite;
use diesel_migrations::{FileBasedMigrations, HarnessWithOutput, MigrationHarness};
use alttp_queue_bot::get_conn;
use alttp_queue_bot::utils::env_var;

fn main() {
    dotenv::dotenv().ok();
    println!("migration_test");
    let migrations = FileBasedMigrations::find_migrations_directory().unwrap();
    let real_migrations: Vec<Box<dyn Migration<Sqlite>>>  = migrations.migrations().unwrap();
    for m in real_migrations {
        let md = m.metadata();
        println!("Migration {}: run_in_tx?: {}", m.name(), md.run_in_transaction());
    }
    // let mig = whatever.migrations().unwrap().pop().unwrap();
    let mut db = get_conn(&env_var("DATABASE_URL")).unwrap();
    let mut s = Vec::new();
    let mut h = HarnessWithOutput::new(&mut db,&mut s);
    let result = h.revert_last_migration(migrations);
    println!("{:?}", result);
}