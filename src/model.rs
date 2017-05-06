
use chrono::*;
use rustc_serialize::json::{Json, Object, ToJson};
use db::DbStored;
use regex::Regex;

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug, PartialEq)]
pub enum QueryActionType {
    Status,
    Announcement,
    Presence,
    All
}

#[derive(Clone, Debug)]
pub struct StatusAction {
    pub action: BaseAction,
    pub user: String,
    pub status: Status
}

#[derive(Clone, Debug, PartialEq)]
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
    pub fn new(note: String, time: i64, user: String, status: Status) -> Self {
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

#[derive(Clone, Debug)]
pub struct AnnouncementAction {
    pub action: BaseAction,
    pub method: AnnouncementMethod,
    pub aid: Option<u64>, // announcement id
    pub user: String,
    pub from: i64,
    pub to: i64,
    pub public: bool
}

#[derive(Clone, Debug,PartialEq)]
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

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
pub struct PresentUser {
    pub name: String,
    pub since: i64,
    pub status: PresentUserStatus
}

#[derive(Clone, Debug, PartialEq)]
pub enum PresentUserStatus {
    Joined,
    Present,
    Left
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
        let json_users: Vec<Json> = self.users.iter().filter_map(|ref user| {
            if user.status == PresentUserStatus::Left {
                None
            } else {
                Some(user.to_json())
            }
        }).collect();
        obj.insert("users".into(), json_users.to_json());
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

pub enum TypedAction {
    Status(StatusAction),
    Announcement(AnnouncementAction),
    Presence(PresenceAction)
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

pub fn parse_time(json: &Json, now: i64) -> Result<i64, String> {
    match json {
        &Json::I64(t) => Ok(t),
        &Json::U64(t) => Ok(t as i64),
        &Json::String(ref s) => {
            parse_time_string(s, now)
        },
        _ => Err("bad time specification (wrong type)".into())
    }
}

fn parse_u64_string(s: &str) -> Result<u64, String> {
    s.parse::<u64>().map_err(|_| "bad integer".into())
}

pub fn parse_time_string(s: &str, now: i64) -> Result<i64, String> {
    match s.parse::<i64>() {
        Ok(i) => {
            return Ok(i);
        },
        Err(_) => {
            if s == "now" {
                return Ok(now);
            }
            let re = Regex::new(r"^now([+-])(\d+)$").unwrap();
            match re.captures(s) {
                None => {
                    return Err("bad time specification".into());
                },
                Some(captures) => {
                    let mut i: i64 = try!(parse_u64_string(captures.at(2).unwrap())) as i64;
                    match captures.at(1) {
                        Some("+") => {
                        },
                        Some("-") => {
                            i = -i;
                        },
                        Some(_) | None => {
                            panic!("should be impossible");
                        }
                    }
                    return Ok(now + i);
                }
            }
        }
    }
}

fn get_public(obj: &Object) -> Result<bool, String> {
    match obj.get("public") {
        None => Ok(false),
        Some(v) => {
            match v.as_boolean() {
                Some(b) => Ok(b),
                None => Err("bad value for 'public'".into())
            }
        }
    }
}


pub enum RequestObject {
    Action(Box<Action>),
    PresenceRequest(String)
}

/*
 * also checks validity of entered values
 */
pub fn json_to_object(json: Json, now: i64) -> Result<RequestObject, String> {
    let obj = json.as_object().unwrap();
    let note = match obj.get("note") {
        Some(j) => {
            let note = j.as_string().unwrap();
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
            Ok(RequestObject::Action(Box::new(StatusAction {
                action: base_action,
                user: user,
                status: Status::from_str(obj.get("status").unwrap().as_string().unwrap()).unwrap()
            })))
        },
        "announcement" => {
            let user = try!(get_checked_user(obj));
            let method = try!(get_method(obj));
            match method {
                AnnouncementMethod::New => {
                    let (from, to) = try!(get_from_to(obj, now));
                    if from < now {
                        return Err("from must be >= now".into());
                    }
                    let public = try!(get_public(obj));
                    Ok(RequestObject::Action(Box::new(AnnouncementAction {
                        action: base_action,
                        user: user,
                        method: method,
                        aid: None,
                        from: from,
                        to: to,
                        public: public
                    })))
                },
                AnnouncementMethod::Mod => {
                    let aid = obj.get("aid").unwrap().as_u64().unwrap();
                    let (from, to) = get_from_to(obj, now).unwrap();
                    let public = try!(get_public(obj));
                    Ok(RequestObject::Action(Box::new(AnnouncementAction {
                        action: base_action,
                        user: user,
                        method: method,
                        aid: Some(aid),
                        from: from,
                        to: to,
                        public: public
                    })))
                }
                AnnouncementMethod::Del => {
                    let aid = obj.get("aid").unwrap().as_u64().unwrap();
                    Ok(RequestObject::Action(Box::new(AnnouncementAction {
                        action: base_action,
                        user: user,
                        method: method,
                        aid: Some(aid),
                        from: 0,      // overwritten by DbStored::store()
                        to: 0,        // overwritten by DbStored::store()
                        public: false // overwritten by DbStored::store()
                    })))
                }
            }
        },
        "presence" => {
            let user = try!(get_checked_user(obj));
            Ok(RequestObject::PresenceRequest(user))
        },
        _ =>
            Err("Unknown action type\n".into())
    }
}

