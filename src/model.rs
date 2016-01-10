
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

pub trait Action: DbStored {
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

pub fn json_to_object(json: Json) -> Box<Action> {
    let obj = json.as_object().unwrap();
    let note = match obj.get("note") {
        Some(j) => String::from(j.as_string().unwrap()),
        None => "".into()
    };
    let base_action = BaseAction::new(note);
    Box::new(match obj.get("type").unwrap().as_string().unwrap() {
        "status" => {
            StatusAction {
                action: base_action,
                user: String::from(obj.get("user").unwrap().as_string().unwrap()),
                status: Status::from_str(obj.get("status").unwrap().as_string().unwrap()).unwrap()
            }
        },
        _ => panic!()
    })
}

