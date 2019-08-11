use std::fs;
use std::path::Path;

use rusqlite::{Connection, Error, Transaction};

use crate::db::DbStored;
use crate::model::*;

pub fn ensure_initialized(path: &Path) -> Result<(), Error> {
    if fs::metadata(path).is_err() {
        println!("creating db at {:?}", path);
        let mut con = Connection::open(path)?;
        let transaction = con.transaction().unwrap();
        create_tables(&transaction);
        insert_initial_status(&transaction);
        insert_initial_presence(&transaction);
        transaction.commit().unwrap();
    }
    Ok(())
}

fn create_tables(tx: &Transaction) {
    /*
     * types:
     *   0: status
     *   1: announcement
     *   2: presence
     */
    tx.execute(
        "CREATE TABLE action (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     time INTEGER NOT NULL,
                     type INTEGER NOT NULL,
                     note TEXT NOT NULL
                 )",
        &[],
    )
    .unwrap();

    /*
     * status:
     *   0: closed
     *   1: private
     *   2: public
     * changed: boolean
     */
    tx.execute(
        "CREATE TABLE status_action (
                 id INTEGER PRIMARY KEY,
                 user TEXT NOT NULL,
                 status INTEGER NOT NULL,
                 changed INTEGER NOT NULL,
                 public_changed INTEGER NOT NULL
             )",
        &[],
    )
    .unwrap();

    /*
     * method:
     *   0: new
     *   1: mod
     *   2: del
     * public: boolean
     */
    tx.execute(
        "CREATE TABLE announcement_action (
                 id INTEGER PRIMARY KEY,
                 method INTEGER,
                 aid INTEGER,
                 user TEXT NOT NULL,
                 'from' INTEGER,
                 'to' INTEGER,
                 public INTEGER
             )",
        &[],
    )
    .unwrap();

    tx.execute(
        "CREATE TABLE presence_action (
                 id INTEGER,
                 user TEXT NOT NULL,
                 since INTEGER
             )",
        &[],
    )
    .unwrap();
}

fn insert_initial_status(tx: &Transaction) {
    let mut status_action = StatusAction::new(
        "initial state".into(),
        0,
        "Hans Acker".into(),
        Status::Closed,
    );
    status_action.store(tx, &None).unwrap();
}

fn insert_initial_presence(tx: &Transaction) {
    let mut presence_action = PresenceAction::new_with_time("initial state".into(), 0, vec![]);
    presence_action.store(tx, &None).unwrap();
}
