
use chrono::*;
use rustc_serialize::json::{Json, Object, ToJson};
use db::DbStored;
use db;

#[derive(Debug)]
pub struct BaseAction {
    pub id: Option<u64>,
    pub time: i64,
    pub note: String
}

impl BaseAction {
    fn new(note: String) -> BaseAction {
        Self::new_with_time(note, UTC::now().timestamp())
    }

    fn new_with_time(note: String, time: i64) -> BaseAction {
        BaseAction {
            id: None,
            time: time,
            note: note
        }
    }

    fn to_json_obj(&self) -> Object {
        let mut obj = Object::new();
        obj.insert("id".into(), self.id.to_json());
        obj.insert("time".into(), self.time.to_json());
        obj.insert("note".into(), self.note.to_json());
        obj
    }
}

#[derive(Debug)]
pub struct StatusAction {
    pub action: BaseAction,
    pub user: String,
    pub status: Status
}

#[derive(Debug,PartialEq)]
pub enum Status {
    Public,
    Private,
    Closed
}

impl Status {
    fn from_str(s: &str) -> Option<Status> {
        match s {
            "public" => Some(Status::Public),
            "private" => Some(Status::Private),
            "closed" => Some(Status::Closed),
            _ => None
        }
    }
}

impl ToJson for Status {
    fn to_json(&self) -> Json {
        Json::String(match self {
            &Status::Public => "public",
            &Status::Private => "private",
            &Status::Closed => "closed"
        }.into())
    }
}

impl StatusAction {
    pub fn new(note: String, user: String, status: Status) -> Self {
        StatusAction {
            action: BaseAction::new(note),
            user: user,
            status: status
        }
    }

    pub fn new_with_time(note: String, time: i64, user: String, status: Status) -> Self {
        StatusAction {
            action: BaseAction::new_with_time(note, time),
            user: user,
            status: status
        }
    }

}

impl ToJson for StatusAction {
    fn to_json(&self) -> Json {
        let mut obj = self.action.to_json_obj();
        obj.insert("type".into(), "status".to_json());
        obj.insert("user".into(), self.user.to_json());
        obj.insert("status".into(), self.status.to_json());
        Json::Object(obj)
    }
}

#[derive(Debug)]
pub struct AnnouncementAction {
    pub action: BaseAction,
    pub method: AnnouncementMethod,
    pub aid: Option<u64>, // announcement id
    pub user: String,
    pub from: i64,
    pub to: i64,
    pub public: bool
}

#[derive(Debug,PartialEq)]
pub enum AnnouncementMethod {
    New,
    Mod,
    Del
}

impl ToJson for AnnouncementMethod {
    fn to_json(&self) -> Json {
        Json::String(match self {
            &AnnouncementMethod::New => "new",
            &AnnouncementMethod::Mod => "mod",
            &AnnouncementMethod::Del => "del"
        }.into())
    }
}

impl ToJson for AnnouncementAction {
    fn to_json(&self) -> Json {
        let mut obj = self.action.to_json_obj();
        obj.insert("type".into(), "announcement".to_json());
        obj.insert("method".into(), self.method.to_json());
        obj.insert("aid".into(), self.aid.to_json());
        obj.insert("user".into(), self.user.to_json());
        obj.insert("from".into(), self.from.to_json());
        obj.insert("to".into(), self.to.to_json());
        obj.insert("public".into(), self.public.to_json());
        Json::Object(obj)
    }
}

#[derive(Debug)]
pub struct PresenceAction {
    pub action: BaseAction,
    pub users: Vec<PresentUser>
}

impl PresenceAction {
    pub fn new(note: String, users: Vec<PresentUser>) -> Self {
        Self::new_with_time(note, UTC::now().timestamp(), users)
    }

    pub fn new_with_time(note: String, time: i64, users: Vec<PresentUser>) -> Self {
        PresenceAction {
            action: BaseAction::new_with_time(note, time),
            users: users
        }
    }
}

#[derive(Debug)]
pub struct PresentUser {
    pub name: String,
    pub since: i64
}

impl PresentUser {
    pub fn new(name: String) -> Self {
        PresentUser {
            name: name,
            since: UTC::now().timestamp()
        }
    }
}

impl ToJson for PresentUser {
    fn to_json(&self) -> Json {
        let mut obj = Object::new();
        obj.insert("name".into(), self.name.to_json());
        obj.insert("since".into(), self.since.to_json());
        Json::Object(obj)
    }
}
impl ToJson for PresenceAction {
    fn to_json(&self) -> Json {
        let mut obj = self.action.to_json_obj();
        obj.insert("type".into(), "presence".to_json());
        obj.insert("users".into(), self.users.to_json());
        Json::Object(obj)
    }
}

pub trait Action: DbStored + ToJson {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction;
}
impl Action for StatusAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction { &self.action }
}
impl Action for AnnouncementAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction { &self.action }
}
impl Action for PresenceAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction { &self.action }
}

impl ToJson for Box<Action> {
    fn to_json(&self) -> Json {
        (**self).to_json()
    }
}


fn get_checked_user(obj: &Object) -> Result<String, String> {
    let user = obj.get("user").unwrap().as_string().unwrap().trim();
    if user.len() == 0 || user.len() > 15 {
        return Err("Bad username, unicode chars, 1 to 15 bytes\n".into());
    }
    Ok(String::from(user))
}

fn get_method(obj: &Object) -> Result<AnnouncementMethod, String> {
    match obj.get("method").unwrap().as_string().unwrap() {
        "new" => Ok(AnnouncementMethod::New),
        "mod" => Ok(AnnouncementMethod::Mod),
        "del" => Ok(AnnouncementMethod::Del),
        _ => {
            Err("bad method".into())
        }
    }
}

fn get_from_to(obj: &Object, now: i64) -> Result<(i64, i64), String> {
    let from = try!(parse_time(obj.get("from").unwrap(), now));
    let to = try!(parse_time(obj.get("to").unwrap(), now));
    if from > to {
        return Err("from must be <= to".into());
    }
    Ok((from, to))
}

fn parse_time(json: &Json, now: i64) -> Result<i64, String> {
    match json {
        &Json::I64(t) => Ok(t),
        &Json::U64(t) => Ok(t as i64),
        &Json::String(ref s) => {
            if s == "now" {
                Ok(now)
            } else {
                Err("bad time specification".into())
            }
        },
        _ => Err("bad time specification (wrong type)".into())
    }
}

/*
 * also checks validity of entered values
 */
pub fn json_to_object(json: Json, now: i64) -> Result<Box<Action>, String> {
    let obj = json.as_object().unwrap();
    let note = match obj.get("note") {
        Some(j) => {
            let note =j.as_string().unwrap();
            if note.len() > 80 {
                return Err("Bad note, unicode chars, maximum 80 bytes\n".into());
            }
            String::from(note)
        }
        None => "".into()
    };
    let base_action = BaseAction::new(note);
    match obj.get("type").unwrap().as_string().unwrap() {
        "status" => {
            let user = try!(get_checked_user(obj));
            Ok(Box::new(StatusAction {
                action: base_action,
                user: user,
                status: Status::from_str(obj.get("status").unwrap().as_string().unwrap()).unwrap()
            }))
        },
        "announcement" => {
            let user = try!(get_checked_user(obj));
            let method = try!(get_method(obj));
            match method {
                AnnouncementMethod::New => {
                    let (from, to) = get_from_to(obj, now).unwrap();
                    if from < now {
                        return Err("from must be >= now".into());
                    }
                    Ok(Box::new(AnnouncementAction {
                        action: base_action,
                        user: user,
                        method: method,
                        aid: None,
                        from: from,
                        to: to,
                        public: false
                    }))
                },
                AnnouncementMethod::Mod => {
                    let aid = obj.get("aid").unwrap().as_u64().unwrap();
                    let (from, to) = get_from_to(obj, now).unwrap();
                    Ok(Box::new(AnnouncementAction {
                        action: base_action,
                        user: user,
                        method: method,
                        aid: Some(aid),
                        from: from,
                        to: to,
                        public: false
                    }))
                }
                AnnouncementMethod::Del => {
                    let aid = obj.get("aid").unwrap().as_u64().unwrap();
                    Ok(Box::new(AnnouncementAction {
                        action: base_action,
                        user: user,
                        method: method,
                        aid: Some(aid),
                        from: 0,      // overwritten by DbStored::store()
                        to: 0,        // overwritten by DbStored::store()
                        public: false // overwritten by DbStored::store()
                    }))
                }
            }
        },
        _ =>
            Err("Unknown action type\n".into())
    }
}

