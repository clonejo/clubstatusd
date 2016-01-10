
use std::path::Path;
use rusqlite::{SqliteConnection, SqliteResult, SqliteError};
use rusqlite::types::{FromSql, ToSql, sqlite3_stmt};
use libc::c_int;
use model::*;

mod init;

pub use db::init::ensure_initialized;

pub type DbCon = SqliteConnection;

pub fn connect(path_str: &str) -> DbCon {
    let path = Path::new(path_str);
    ensure_initialized(path);
    SqliteConnection::open(path).unwrap()
}

pub trait DbStored {
    fn store(&mut self, con: &DbCon) -> Option<u64>;
}

impl DbStored for BaseAction {
    fn store(&mut self, con: &DbCon) -> Option<u64> {
        con.execute("INSERT INTO action (time, type, note) VALUES (?, ?, ?)",
                    &[&self.time, &0, &self.note]).unwrap();
        let action_id = con.last_insert_rowid() as u64;
        Some(action_id)
    }
}

/*
 * Status
 */

impl DbStored for StatusAction {
    fn store(&mut self, con: &DbCon) -> Option<u64> {
        match self.action.id {
            None => {
                let transaction = con.transaction().unwrap();
                let changed = match status::get_last(con) {
                    Ok(current_status) => current_status.status != self.status,
                    Err(_) => true
                };
                let action_id = self.action.store(con).unwrap();
                con.execute("INSERT INTO status_action (id, user, status, changed) VALUES (?, ?, ?, ?)",
                            &[&(action_id as i64), &self.user, &self.status, &(changed as i64)]).unwrap();
                self.action.id = Some(action_id);
                transaction.commit().unwrap();
                Some(action_id)
            },
            Some(_) =>
                None
        }
    }
}

impl FromSql for Status {
    unsafe fn column_result(stmt: *mut sqlite3_stmt, col: c_int) -> SqliteResult<Status> {
        match FromSql::column_result(stmt, col) {
            Ok(i) => match i {
                0 => Ok(Status::Closed),
                1 => Ok(Status::Private),
                2 => Ok(Status::Public),
                _ => Err(SqliteError{code: 1, message: "unknown Status".into()})
            },
            Err(e) => Err(e)
        }
    }
}
impl ToSql for Status {
    unsafe fn bind_parameter(&self, stmt: *mut sqlite3_stmt, col: c_int) -> c_int {
        match self {
            &Status::Public => 2,
            &Status::Private => 1,
            &Status::Closed => 0
        }.bind_parameter(stmt, col)
    }
}

pub mod status {
    use super::*;
    use super::super::model::*;
    use rusqlite::{SqliteResult, SqliteRow};

    fn row_to_status_action(row: SqliteRow) -> StatusAction {
        StatusAction{
            action: BaseAction {
                id: Some(row.get::<i64>(0) as u64),
                time: row.get(1),
                note: row.get(3)
            },
            user: row.get(5),
            status: row.get(6)
        }
    }

    pub fn get_last(con: &DbCon) -> SqliteResult<StatusAction> {
        con.query_row("SELECT * FROM action JOIN status_action WHERE action.type = 0 AND \
                       action.id = status_action.id \
                       ORDER BY action.id DESC LIMIT 1",
                      &[],
                      row_to_status_action)
    }

    pub fn get_last_changed(con: &DbCon) -> SqliteResult<StatusAction> {
        con.query_row("SELECT * FROM action JOIN status_action WHERE action.type = 0 AND \
                       action.id = status_action.id AND status_action.changed = 1 \
                       ORDER BY action.id DESC LIMIT 1",
                      &[],
                      row_to_status_action)
    }
}


/*
 * Announcements
 */

impl DbStored for AnnouncementAction {
    fn store(&mut self, con: &SqliteConnection) -> Option<u64> {
        match self.action.id {
            None => {
                match self.method {
                    AnnouncementMethod::New =>
                        match self.aid {
                            None => {
                                let transaction = con.transaction().unwrap();
                                let action_id = self.action.store(con).unwrap();
                                con.execute("INSERT INTO announcement_action (id, method, aid, user, 'from', 'to', public) VALUES (?, ?, ?, ?, ?, ?, ?)",
                                    &[&(action_id as i64), &0, &(action_id as i64),
                                      &self.user, &self.from, &self.to, &(self.public as i64)]).unwrap();
                                self.action.id = Some(action_id);
                                self.aid = Some(action_id);
                                transaction.commit().unwrap();
                                Some(action_id)
                            },
                            Some(_) =>
                                None
                        },
                    AnnouncementMethod::Mod =>
                        match self.aid {
                            None =>
                                None,
                            Some(aid) => {
                                let transaction = con.transaction().unwrap();
                                // check if last action is method=new|mod
                                let action_id = self.action.store(con).unwrap();
                                con.execute("INSERT INTO announcement_action (id, method, aid, user, 'from', 'to', public) VALUES (?, ?, ?, ?, ?, ?, ?)",
                                    &[&(action_id as i64), &0, &(aid as i64),
                                      &self.user, &self.from, &self.to, &(self.public as i64)]).unwrap();
                                self.action.id = Some(action_id);
                                transaction.commit().unwrap();
                                Some(action_id)
                            }
                        },
                    AnnouncementMethod::Del =>
                        match self.aid {
                            None =>
                                None,
                            Some(aid) => {
                                let transaction = con.transaction().unwrap();
                                // check if last action is method=new|mod
                                let action_id = self.action.store(con).unwrap();
                                con.execute("INSERT INTO announcement_action (id, method, aid, user, 'from', 'to', public) VALUES (?, ?, ?, ?, ?, ?, ?)",
                                    &[&(action_id as i64), &0, &(aid as i64),
                                      &self.user, &self.from, &self.to, &(self.public as i64)]).unwrap();
                                self.action.id = Some(action_id);
                                transaction.commit().unwrap();
                                Some(action_id)
                            }
                        }
                }

            },
            Some(_) =>
                None
        }
    }
}

impl FromSql for AnnouncementMethod {
    unsafe fn column_result(stmt: *mut sqlite3_stmt, col: c_int) -> SqliteResult<AnnouncementMethod> {
        match FromSql::column_result(stmt, col) {
            Ok(i) => match i {
                0 => Ok(AnnouncementMethod::New),
                1 => Ok(AnnouncementMethod::Mod),
                2 => Ok(AnnouncementMethod::Del),
                _ => Err(SqliteError{code: 1, message: "unknown AnnouncementMethod".into()})
            },
            Err(e) => Err(e)
        }
    }
}
impl ToSql for AnnouncementMethod {
    unsafe fn bind_parameter(&self, stmt: *mut sqlite3_stmt, col: c_int) -> c_int {
        match self {
            &AnnouncementMethod::New => 0,
            &AnnouncementMethod::Mod => 1,
            &AnnouncementMethod::Del => 2
        }.bind_parameter(stmt, col)
    }
}

pub mod announcements {
    use super::*;
    use super::super::model::*;
    use rusqlite::{SqliteResult, SqliteError};

    pub fn get_last(aid: u64, con: &DbCon) -> SqliteResult<Option<AnnouncementAction>> {
        let res = con.query_row("SELECT * FROM action JOIN announcement_action WHERE action.type = 1 AND \
                                 action.id = announcement_action.id AND announcement_action.aid = ? \
                                 ORDER BY action.id DESC LIMIT 1",
                                &[&(aid as i64)],
                                |row| {
                                    AnnouncementAction{
                                        action: BaseAction {
                                            id: Some(row.get::<i64>(0) as u64),
                                                time: row.get(1),
                                            note: row.get(3)
                                        },
                                        method: row.get(5),
                                        aid: Some(row.get::<i64>(6) as u64),
                                        user: row.get(7),
                                        from: row.get(8),
                                        to: row.get(9),
                                        public: match row.get(10) {
                                            0 => false,
                                            1 => true,
                                            _ => panic!()
                                        }
                                    }
                                });
        match res {
            Ok(announcement_action) => Ok(Some(announcement_action)),
            Err(SqliteError{code: 27, message: _}) => Ok(None),
            Err(e) => Err(e)
        }
    }
}


/*
 * Presence
 */

impl DbStored for PresenceAction {
    fn store(&mut self, con: &SqliteConnection) -> Option<u64> {
        match self.action.id {
            None => {
                let transaction = con.transaction().unwrap();
                con.execute("INSERT INTO action (time, type, note) VALUES (?, ?, ?)",
                            &[&self.action.time, &2, &self.action.note]).unwrap();
                let action_id = con.last_insert_rowid() as u64;
                for user in self.users.iter() {
                    con.execute("INSERT INTO presence_action (id, user, since) VALUES (?, ?, ?)",
                                &[&(action_id as i64), &user.name, &user.since]).unwrap();
                }
                self.action.id = Some(action_id);
                transaction.commit().unwrap();
                Some(action_id)
            },
            Some(_) =>
                None
        }
    }
}

pub mod presence {
    use super::*;
    use super::super::model::*;
    use rusqlite::{SqliteResult};

    pub fn get_last(con: &DbCon) -> SqliteResult<PresenceAction> {
        let action_res = con.query_row("SELECT * FROM action WHERE type = 2 ORDER BY id DESC LIMIT 1",
                                       &[],
                                       |row| {
                                           BaseAction {
                                               id: Some(row.get::<i64>(0) as u64),
                                               time: row.get(1),
                                               note: row.get(3)
                                           }
                                       });
        match action_res {
            Ok(action) => {
                let mut stmt = con.prepare("SELECT user, since FROM presence_action WHERE id = ?").unwrap();
                let users_iter = stmt.query_map(&[&(action.id.unwrap() as i64)], |row| {
                    PresentUser{name: row.get(0), since: row.get(1)}
                }).unwrap();
                let users = users_iter.map(|user| { user.unwrap() }).collect();
                Ok(
                    PresenceAction{
                        action: action,
                        users: users
                    }
                  )
            },
            Err(e) =>
                Err(e)
        }
    }
}

