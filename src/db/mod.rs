use std::path::Path;
use std::sync::mpsc::Sender;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Value, ValueRef};
use rusqlite::{Connection, Error, Transaction};

use crate::api::{IdExpr, RangeExpr, Take};
use crate::model::*;

mod init;

pub use crate::db::init::ensure_initialized;

pub type DbCon = Connection;

pub fn connect(path_str: &str) -> Result<DbCon, Error> {
    let path = Path::new(path_str);
    ensure_initialized(path).unwrap();
    Connection::open(path)
}

pub trait DbStored {
    fn store(&mut self, con: &Transaction, mqtt: &Option<Sender<TypedAction>>) -> Option<u64>;
}

pub trait DbStoredTyped {
    fn store(&mut self, type_: i64, con: &DbCon) -> Option<u64>;
}

impl DbStoredTyped for BaseAction {
    fn store(&mut self, type_: i64, con: &DbCon) -> Option<u64> {
        con.execute(
            "INSERT INTO action (time, type, note) VALUES (?, ?, ?)",
            &[&self.time, &type_, &self.note],
        )
        .unwrap();
        let action_id = con.last_insert_rowid() as u64;
        Some(action_id)
    }
}

/*
 * Status
 */

impl DbStored for StatusAction {
    fn store(&mut self, tx: &Transaction, mqtt: &Option<Sender<TypedAction>>) -> Option<u64> {
        match self.action.id {
            None => {
                let (changed, public_changed) = match status::get_last(tx) {
                    Ok(current_status) => {
                        let curr_status_pub = match current_status.status {
                            Status::Public => Status::Public,
                            _ => Status::Closed,
                        };
                        let new_status_pub = match self.status {
                            Status::Public => Status::Public,
                            _ => Status::Closed,
                        };
                        (
                            current_status.status != self.status,
                            curr_status_pub != new_status_pub,
                        )
                    }
                    Err(_) => (true, true),
                };
                let action_id = DbStoredTyped::store(&mut self.action, 0, tx).unwrap();
                tx.execute(
                    "INSERT INTO status_action (id, user, status, changed, public_changed) \
                     VALUES (?, ?, ?, ?, ?)",
                    &[
                        &(action_id as i64),
                        &self.user,
                        &self.status,
                        &(changed as i64),
                        &(public_changed as i64),
                    ],
                )
                .unwrap();
                self.action.id = Some(action_id);
                match mqtt {
                    Some(ref m) => {
                        m.send(TypedAction::Status(self.clone())).unwrap();
                    }
                    None => {}
                }
                Some(action_id)
            }
            Some(_) => None,
        }
    }
}

impl FromSql for Status {
    fn column_result(value: ValueRef) -> FromSqlResult<Status> {
        match FromSql::column_result(value) {
            Ok(i) => match i {
                0 => Ok(Status::Closed),
                1 => Ok(Status::Private),
                2 => Ok(Status::Public),
                _ => Err(FromSqlError::Other("unknown Status".into())),
            },
            Err(e) => Err(e),
        }
    }
}
impl ToSql for Status {
    fn to_sql(&self) -> Result<ToSqlOutput, Error> {
        let i = match self {
            Status::Public => 2,
            Status::Private => 1,
            Status::Closed => 0,
        };
        Ok(ToSqlOutput::Owned(Value::Integer(i)))
    }
}

pub mod status {
    use super::*;
    use rusqlite::{Error, Row};

    fn row_to_status_action(row: &Row) -> StatusAction {
        StatusAction {
            action: BaseAction {
                id: Some(row.get::<_, i64>(0) as u64),
                time: row.get(1),
                note: row.get(3),
            },
            user: row.get(5),
            status: row.get(6),
        }
    }

    pub fn get_by_id(id: u64, con: &DbCon) -> Result<StatusAction, Error> {
        con.query_row(
            "SELECT * FROM action JOIN status_action WHERE \
             action.id = ? AND status_action.id = ?",
            &[&(id as i64), &(id as i64)],
            row_to_status_action,
        )
    }

    pub fn get_last(con: &DbCon) -> Result<StatusAction, Error> {
        con.query_row(
            "SELECT * FROM action JOIN status_action WHERE action.type = 0 AND \
             action.id = status_action.id \
             ORDER BY action.id DESC LIMIT 1",
            &[],
            row_to_status_action,
        )
    }

    pub fn get_last_changed(con: &DbCon) -> Result<StatusAction, Error> {
        con.query_row(
            "SELECT * FROM action JOIN status_action WHERE action.type = 0 AND \
             action.id = status_action.id AND status_action.changed = 1 \
             ORDER BY action.id DESC LIMIT 1",
            &[],
            row_to_status_action,
        )
    }

    pub fn get_last_changed_public(con: &DbCon) -> Result<StatusAction, Error> {
        con.query_row(
            "SELECT * FROM action JOIN status_action WHERE action.type = 0 AND \
             action.id = status_action.id AND status_action.public_changed = 1 \
             ORDER BY action.id DESC LIMIT 1",
            &[],
            row_to_status_action,
        )
    }
}

/*
 * Announcements
 */

impl DbStored for AnnouncementAction {
    fn store(&mut self, tx: &Transaction, mqtt: &Option<Sender<TypedAction>>) -> Option<u64> {
        match self.action.id {
            None => {
                match self.method {
                    AnnouncementMethod::New => match self.aid {
                        None => {
                            let action_id = DbStoredTyped::store(&mut self.action, 1, tx).unwrap();
                            tx.execute(
                                "INSERT INTO announcement_action \
                                 (id, method, aid, user, \"from\", \"to\", public) VALUES \
                                 (?, ?, ?, ?, ?, ?, ?)",
                                &[
                                    &(action_id as i64),
                                    &0,
                                    &(action_id as i64),
                                    &self.user,
                                    &self.from,
                                    &self.to,
                                    &(self.public as i64),
                                ],
                            )
                            .unwrap();
                            self.action.id = Some(action_id);
                            self.aid = Some(action_id);
                            if let Some(ref m) = mqtt {
                                m.send(TypedAction::Announcement(self.clone())).unwrap();
                            }
                            Some(action_id)
                        }
                        Some(_) => None,
                    },
                    AnnouncementMethod::Mod => match self.aid {
                        None => None,
                        Some(aid) => {
                            // check if last action is method=new|mod
                            let _last_action = match announcements::get_last(aid, tx).unwrap() {
                                None => return None,
                                Some(AnnouncementAction {
                                    method: AnnouncementMethod::Del,
                                    ..
                                }) => return None,
                                Some(a) => a,
                            };
                            let action_id = DbStoredTyped::store(&mut self.action, 1, tx).unwrap();
                            tx.execute("INSERT INTO announcement_action (id, method, aid, user, 'from', 'to', public) VALUES (?, ?, ?, ?, ?, ?, ?)",
                                    &[&(action_id as i64), &1, &(aid as i64),
                                      &self.user, &self.from, &self.to, &(self.public as i64)]).unwrap();
                            self.action.id = Some(action_id);
                            if let Some(ref m) = mqtt {
                                m.send(TypedAction::Announcement(self.clone())).unwrap();
                            }
                            Some(action_id)
                        }
                    },
                    AnnouncementMethod::Del => match self.aid {
                        None => None,
                        Some(aid) => {
                            // check if last action is method=new|mod
                            let last_action = match announcements::get_last(aid, tx).unwrap() {
                                None => return None,
                                Some(AnnouncementAction {
                                    method: AnnouncementMethod::Del,
                                    ..
                                }) => return None,
                                Some(a) => a,
                            };
                            self.action.note = last_action.action.note;
                            self.from = last_action.from;
                            self.to = last_action.to;
                            self.public = last_action.public;
                            let action_id = DbStoredTyped::store(&mut self.action, 1, tx).unwrap();
                            tx.execute("INSERT INTO announcement_action (id, method, aid, user, 'from', 'to', public) VALUES (?, ?, ?, ?, ?, ?, ?)",
                                    &[&(action_id as i64), &2, &(aid as i64),
                                      &self.user, &self.from, &self.to, &(self.public as i64)]).unwrap();
                            self.action.id = Some(action_id);
                            if let Some(m) = mqtt {
                                m.send(TypedAction::Announcement(self.clone())).unwrap();
                            }
                            Some(action_id)
                        }
                    },
                }
            }
            Some(_) => None,
        }
    }
}

impl FromSql for AnnouncementMethod {
    fn column_result(value: ValueRef) -> FromSqlResult<AnnouncementMethod> {
        match FromSql::column_result(value) {
            Result::Ok(i) => match i {
                0 => Result::Ok(AnnouncementMethod::New),
                1 => Result::Ok(AnnouncementMethod::Mod),
                2 => Result::Ok(AnnouncementMethod::Del),
                _ => Result::Err(FromSqlError::Other("unknown AnnouncementMethod".into())),
            },
            Result::Err(e) => Result::Err(e),
        }
    }
}
impl ToSql for AnnouncementMethod {
    fn to_sql(&self) -> Result<ToSqlOutput, Error> {
        let i = match self {
            AnnouncementMethod::New => 0,
            AnnouncementMethod::Mod => 1,
            AnnouncementMethod::Del => 2,
        };
        Ok(ToSqlOutput::Owned(Value::Integer(i)))
    }
}

pub mod announcements {
    use super::*;
    use chrono::*;
    use rusqlite::{Error, Row};

    fn row_to_announcement_action(row: &Row) -> AnnouncementAction {
        AnnouncementAction {
            action: BaseAction {
                id: Some(row.get::<_, i64>(0) as u64),
                time: row.get(1),
                note: row.get(3),
            },
            method: row.get(5),
            aid: Some(row.get::<_, i64>(6) as u64),
            user: row.get(7),
            from: row.get(8),
            to: row.get(9),
            public: match row.get(10) {
                0 => false,
                1 => true,
                _ => panic!(),
            },
        }
    }

    pub fn get_by_id(id: u64, con: &DbCon) -> Result<AnnouncementAction, Error> {
        con.query_row(
            "SELECT * FROM action JOIN announcement_action WHERE action.type = 1 AND \
             action.id = ? AND announcement_action.id = ?",
            &[&(id as i64), &(id as i64)],
            row_to_announcement_action,
        )
    }

    pub fn get_last(aid: u64, con: &DbCon) -> Result<Option<AnnouncementAction>, Error> {
        let res = con.query_row(
            "SELECT * FROM action JOIN announcement_action WHERE action.type = 1 AND \
             action.id = announcement_action.id AND announcement_action.aid = ? \
             ORDER BY action.id DESC LIMIT 1",
            &[&(aid as i64)],
            row_to_announcement_action,
        );
        match res {
            Ok(announcement_action) => Ok(Some(announcement_action)),
            Err(Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn get_current(con: &DbCon) -> Result<Vec<AnnouncementAction>, Error> {
        let mut stmt = con
            .prepare(
                "SELECT * FROM action JOIN announcement_action WHERE \
                 action.id IN ( \
                 SELECT max(id) FROM announcement_action GROUP BY aid \
                 ) AND \
                 action.id = announcement_action.id AND \
                 ? <= \"to\" AND \
                 announcement_action.method != 2 \
                 ORDER BY \"from\" LIMIT 30",
            )
            .unwrap();
        let now = Utc::now().timestamp();
        let actions_iter = stmt.query_map(&[&now], row_to_announcement_action).unwrap();
        let actions: Vec<AnnouncementAction> = actions_iter.map(|action| action.unwrap()).collect();
        Ok(actions)
    }

    pub fn get_current_public(con: &DbCon) -> Result<Vec<AnnouncementAction>, Error> {
        let mut stmt = con
            .prepare(
                "SELECT * FROM action JOIN announcement_action WHERE \
                 action.id IN ( \
                 SELECT max(id) FROM announcement_action GROUP BY aid \
                 ) AND \
                 action.id = announcement_action.id AND \
                 ? <= \"to\" AND \
                 announcement_action.method != 2 AND \
                 announcement_action.public = 1 \
                 ORDER BY \"from\" LIMIT 30",
            )
            .unwrap();
        let now = Utc::now().timestamp();
        let actions_iter = stmt.query_map(&[&now], row_to_announcement_action).unwrap();
        let actions: Vec<AnnouncementAction> = actions_iter.map(|action| action.unwrap()).collect();
        Ok(actions)
    }
}

/*
 * Presence
 */

impl DbStored for PresenceAction {
    fn store(&mut self, tx: &Transaction, mqtt: &Option<Sender<TypedAction>>) -> Option<u64> {
        match self.action.id {
            None => {
                tx.execute(
                    "INSERT INTO action (time, type, note) VALUES (?, ?, ?)",
                    &[&self.action.time, &2, &self.action.note],
                )
                .unwrap();
                let action_id = tx.last_insert_rowid() as u64;
                for user in self.users.iter() {
                    if user.status != PresentUserStatus::Left {
                        tx.execute(
                            "INSERT INTO presence_action (id, user, since) VALUES (?, ?, ?)",
                            &[&(action_id as i64), &user.name, &user.since],
                        )
                        .unwrap();
                    }
                }
                self.action.id = Some(action_id);
                match mqtt {
                    Some(ref m) => {
                        m.send(TypedAction::Presence(self.clone())).unwrap();
                    }
                    None => {}
                }
                Some(action_id)
            }
            Some(_) => None,
        }
    }
}

pub mod presence {
    use super::*;
    use chrono::Utc;
    use rusqlite::Row;
    use std::collections::HashMap;
    use std::iter::FromIterator;
    use std::sync::mpsc::{channel, Sender, TryRecvError};
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn row_to_base_action(row: &Row) -> BaseAction {
        BaseAction {
            id: Some(row.get::<_, i64>(0) as u64),
            time: row.get(1),
            note: row.get(3),
        }
    }

    fn get_by_base_action(action: BaseAction, con: &DbCon) -> Result<PresenceAction, Error> {
        let mut stmt = con
            .prepare("SELECT user, since FROM presence_action WHERE id = ?")
            .unwrap();
        let users_iter = stmt
            .query_map(&[&(action.id.unwrap() as i64)], |row| PresentUser {
                name: row.get(0),
                since: row.get(1),
                status: PresentUserStatus::Present,
            })
            .unwrap();
        let mut users: Vec<PresentUser> = users_iter.map(|user| user.unwrap()).collect();
        users.sort_by_key(|u| u.name.clone());
        Ok(PresenceAction { action, users })
    }

    pub fn get_by_id(id: u64, con: &DbCon) -> Result<PresenceAction, Error> {
        let action_res = con.query_row(
            "SELECT * FROM action WHERE id = ? AND type = 2",
            &[&(id as i64)],
            row_to_base_action,
        );
        match action_res {
            Ok(action) => get_by_base_action(action, con),
            Err(e) => Err(e),
        }
    }

    pub fn get_last(con: &DbCon) -> Result<PresenceAction, Error> {
        let action_res = con.query_row(
            "SELECT * FROM action WHERE type = 2 ORDER BY id DESC LIMIT 1",
            &[],
            row_to_base_action,
        );
        match action_res {
            Ok(action) => get_by_base_action(action, con),
            Err(e) => Err(e),
        }
    }

    pub fn start_tracker(
        shared_con: Arc<Mutex<DbCon>>,
        mqtt: Option<Sender<TypedAction>>,
    ) -> Sender<String> {
        let (tx, rx) = channel::<String>();
        thread::Builder::new()
            .name(String::from("presence_tracker"))
            .spawn(move || {
                #[derive(Debug)]
                struct UserPresence {
                    since: i64,
                    last_seen: i64,
                    status: PresentUserStatus,
                }
                let last_action = {
                    let con = shared_con.lock().unwrap();
                    get_last(&*con).unwrap()
                };
                let mut now = Utc::now().timestamp();
                let mut users: HashMap<String, UserPresence> = HashMap::new();
                for user in last_action.users {
                    users.insert(
                        user.name,
                        UserPresence {
                            since: user.since,
                            last_seen: now,
                            status: PresentUserStatus::Present,
                        },
                    );
                }
                let mut changed = false;
                loop {
                    // scrape users with status=left
                    let new_users: HashMap<String, UserPresence> =
                        HashMap::from_iter(users.drain().filter(|&(_, ref presence)| {
                            let keep = presence.status != PresentUserStatus::Left;
                            if !keep {
                                changed = true;
                            }
                            keep
                        }));
                    users = new_users;

                    // presence requests time out after 15min + time slept
                    // set these users' status to left
                    for (ref _user, ref mut presence) in users.iter_mut() {
                        // use values_mut() when stable
                        if presence.last_seen + 15 * 60 <= now {
                            presence.status = PresentUserStatus::Left;
                            changed = true;
                        }
                    }

                    // create action
                    let mut present_users = Vec::new();
                    for (username, presence) in users.iter() {
                        present_users.push(PresentUser {
                            name: username.clone(),
                            since: presence.since,
                            status: presence.status.clone(),
                        });
                    }
                    if changed {
                        let mut con = shared_con.lock().unwrap();
                        let mut presence_action =
                            PresenceAction::new(String::from(""), present_users);
                        let transaction = con.transaction().unwrap();
                        presence_action.store(&transaction, &mqtt);
                        transaction.commit().unwrap();
                        changed = false;
                    }

                    // switch users with status=joined to present
                    for (ref _user, ref mut presence) in users.iter_mut() {
                        // use values_mut() when stable
                        if presence.status == PresentUserStatus::Joined {
                            presence.status = PresentUserStatus::Present;
                            changed = true;
                        }
                    }

                    thread::sleep(Duration::new(20, 0)); // create one presence action every 20s

                    // add requests to user list
                    now = Utc::now().timestamp();
                    loop {
                        match rx.try_recv() {
                            Ok(username) => {
                                let presence = users.entry(username).or_insert_with(|| {
                                    changed = true;
                                    UserPresence {
                                        since: now,
                                        last_seen: now,
                                        status: PresentUserStatus::Joined,
                                    }
                                });
                                presence.last_seen = now;
                            }
                            Err(TryRecvError::Empty) => {
                                break;
                            }
                            Err(TryRecvError::Disconnected) => {
                                return;
                            }
                        }
                    }
                }
            })
            .unwrap();
        tx
    }
}

pub fn query(
    type_: QueryActionType,
    id: RangeExpr<IdExpr>,
    time: RangeExpr<i64>,
    count: u64,
    take: Take,
    con: &mut DbCon,
) -> Result<Vec<Box<dyn Action>>, Error> {
    let mut query_str = String::from("SELECT id, type FROM action WHERE");

    // for livetime reasons we need to define these variables before params:
    let id1;
    let id2;
    let time1;
    let time2;
    let count = count as i64;
    let type_int;

    let mut params = Vec::<&dyn ToSql>::new();

    match id {
        RangeExpr::Single(IdExpr::Int(i)) => {
            id1 = i as i64;
            query_str.push_str(" id=?");
            params.push(&id1);
        }
        RangeExpr::Single(IdExpr::Last) => {
            query_str.push_str(" 1");
        }
        RangeExpr::Range(IdExpr::Int(i1), IdExpr::Int(i2)) => {
            id1 = i1 as i64;
            id2 = i2 as i64;
            query_str.push_str(" id >= ? AND id <= ?");
            params.push(&id1);
            params.push(&id2);
        }
        RangeExpr::Range(IdExpr::Int(i1), IdExpr::Last) => {
            id1 = i1 as i64;
            query_str.push_str(" id >= ?");
            params.push(&id1);
        }
        RangeExpr::Range(_, _) => {
            panic!("this case should be unreachable");
        }
    }

    match time {
        RangeExpr::Single(t) => {
            time1 = t;
            time2 = t;
        }
        RangeExpr::Range(t1, t2) => {
            time1 = t1;
            time2 = t2;
        }
    };
    query_str.push_str(" AND \"time\" >= ? AND \"time\" <= ?");
    params.push(&time1);
    params.push(&time2);

    if type_ != QueryActionType::All {
        type_int = match type_ {
            QueryActionType::Status => 0,
            QueryActionType::Announcement => 1,
            QueryActionType::Presence => 2,
            _ => panic!(), // impossible
        };
        query_str.push_str(" AND type=?");
        params.push(&type_int);
    }

    query_str.push_str(" ORDER BY id ");
    query_str.push_str(match take {
        Take::First => "ASC",
        Take::Last => "DESC",
    });
    query_str.push_str(" LIMIT ?");
    params.push(&count);

    let mut stmt = con.prepare(&query_str[..]).unwrap();
    let actions_iter = stmt
        .query_map(&*params, |row| -> Box<dyn Action> {
            match row.get(1) {
                0 => Box::new(status::get_by_id(row.get::<_, i64>(0) as u64, &con).unwrap())
                    as Box<dyn Action>,
                1 => Box::new(announcements::get_by_id(row.get::<_, i64>(0) as u64, &con).unwrap())
                    as Box<dyn Action>,
                2 => Box::new(presence::get_by_id(row.get::<_, i64>(0) as u64, &con).unwrap())
                    as Box<dyn Action>,
                id => panic!("unknown action type in db: {}", id),
            }
        })
        .unwrap();
    let mut actions: Vec<Box<dyn Action>> = actions_iter.map(|res| res.unwrap()).collect();
    if take == Take::Last {
        actions.reverse();
    }
    Ok(actions)
}
