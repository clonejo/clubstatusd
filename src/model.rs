
use chrono::*;
use rustc_serialize::json::{Json, Object, ToJson};
use db::DbStored;

#[derive(Debug)]
pub struct BaseAction {
    pub id: Option<u64>,
    pub time: i64,
    pub note: String
}

impl BaseAction {
    fn new(note: String) -> BaseAction {
        BaseAction {
            id: None,
            time: UTC::now().timestamp(),
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

#[derive(Debug)]
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
        PresenceAction {
            action: BaseAction {
                id: Option::None,
                time: UTC::now().timestamp(),
                note: note
            },
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


pub fn json_to_object(json: Json) -> Result<Box<Action>, String> {
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
            let user = obj.get("user").unwrap().as_string().unwrap().trim();
            if user.len() == 0 || user.len() > 15 {
                return Err("Bad username, unicode chars, 1 to 15 bytes\n".into());
            }
            Ok(Box::new(StatusAction {
                action: base_action,
                user: String::from(user),
                status: Status::from_str(obj.get("status").unwrap().as_string().unwrap()).unwrap()
            }))
        },
        _ =>
            Err("Unknown action type\n".into())
    }
}

