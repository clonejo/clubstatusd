use std::path::Path;
use rusqlite::SqliteConnection;
use std::fs;
use model::*;
use db::DbStored;

pub fn ensure_initialized(path: &Path) -> SqliteConnection {
    match fs::metadata(path) {
        Err(_) => {
            let con = SqliteConnection::open(path).unwrap();
            create_tables(&con);
            insert_initial_status(&con);
            insert_initial_presence(&con);
            con
        }
        Ok(_) =>
            SqliteConnection::open(path).unwrap()
    }
}

fn create_tables(con: &SqliteConnection) {
    /*
     * types:
     *   0: status
     *   1: announcement
     *   2: presence
     */
    con.execute("CREATE TABLE action (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     time INTEGER NOT NULL,
                     type INTEGER NOT NULL,
                     note TEXT NOT NULL
                 )", &[]).unwrap();

    /*
 * status:
 *   0: closed
 *   1: private
 *   2: public
 * changed: boolean
 */
con.execute("CREATE TABLE status_action (
                 id INTEGER PRIMARY KEY,
                 user TEXT NOT NULL,
                 status INTEGER NOT NULL,
                 changed INTEGER NOT NULL,
                 public_changed INTEGER NOT NULL
             )", &[]).unwrap();

/*
 * method:
 *   0: new
 *   1: mod
 *   2: del
 * public: boolean
 */
con.execute("CREATE TABLE announcement_action (
                 id INTEGER PRIMARY KEY,
                 method INTEGER,
                 aid INTEGER,
                 user TEXT NOT NULL,
                 'from' INTEGER,
                 'to' INTEGER,
                 public INTEGER
             )", &[]).unwrap();

con.execute("CREATE TABLE presence_action (
                 id INTEGER,
                 user TEXT NOT NULL,
                 since INTEGER
             )", &[]).unwrap();
}

fn insert_initial_status(con: &SqliteConnection) {
    let mut status_action = StatusAction::new("initial state".into(), "Hans Acker".into(), Status::Closed);
    status_action.store(con).unwrap();
}

fn insert_initial_presence(con: &SqliteConnection) {
    let mut presence_action = PresenceAction::new(
        "initial state".into(),
        vec![PresentUser::new("Hans Acker".into())]);
    presence_action.store(con).unwrap();
    let mut presence_action2 = PresenceAction::new(
        "Hans Acker left".into(),
        vec![]);
    presence_action2.store(con).unwrap();
}
