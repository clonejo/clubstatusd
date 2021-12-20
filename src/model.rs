use chrono::Utc;
use rocket::request::FromParam;
use rocket::serde::{Deserialize, Serialize};
use rustc_serialize::json::{Json, Object, ToJson};

use crate::db::DbStored;

#[derive(Clone, Debug, Serialize)]
pub struct BaseAction {
    pub id: Option<u64>,
    pub time: i64,
    pub note: String,
}

impl BaseAction {
    fn new_with_time(note: String, time: i64) -> BaseAction {
        BaseAction {
            id: None,
            time,
            note,
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
    All,
}
impl<'a> FromParam<'a> for QueryActionType {
    type Error = &'static str;
    fn from_param(param: &'a str) -> Result<Self, Self::Error> {
        match param {
            "status" => Ok(QueryActionType::Status),
            "announcement" => Ok(QueryActionType::Announcement),
            "presence" => Ok(QueryActionType::Presence),
            "all" => Ok(QueryActionType::All),
            _ => Err("action type must one of  status | announcement | presence | all"),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct StatusAction {
    #[serde(flatten)]
    pub action: BaseAction,
    pub user: String,
    pub status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Public,
    Private,
    Closed,
}

impl StatusAction {
    pub fn new(note: String, time: i64, user: String, status: Status) -> Self {
        StatusAction {
            action: BaseAction::new_with_time(note, time),
            user,
            status,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AnnouncementAction {
    #[serde(flatten)]
    pub action: BaseAction,
    pub method: AnnouncementMethod,
    pub aid: Option<u64>, // announcement id
    pub user: String,
    pub from: i64,
    pub to: i64,
    pub public: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnnouncementMethod {
    New,
    Mod,
    Del,
}

impl ToJson for AnnouncementMethod {
    fn to_json(&self) -> Json {
        Json::String(
            match self {
                AnnouncementMethod::New => "new",
                AnnouncementMethod::Mod => "mod",
                AnnouncementMethod::Del => "del",
            }
            .into(),
        )
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

#[derive(Clone, Debug, Serialize)]
pub struct PresenceAction {
    #[serde(flatten)]
    pub action: BaseAction,
    pub users: Vec<PresentUser>,
}

impl PresenceAction {
    pub fn new(note: String, users: Vec<PresentUser>) -> Self {
        Self::new_with_time(note, Utc::now().timestamp(), users)
    }

    pub fn new_with_time(note: String, time: i64, users: Vec<PresentUser>) -> Self {
        PresenceAction {
            action: BaseAction::new_with_time(note, time),
            users,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PresentUser {
    pub name: String,
    pub since: i64,
    pub status: PresentUserStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentUserStatus {
    Joined,
    Present,
    Left,
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
        let json_users: Vec<Json> = self
            .users
            .iter()
            .filter_map(|user| {
                if user.status == PresentUserStatus::Left {
                    None
                } else {
                    Some(user.to_json())
                }
            })
            .collect();
        obj.insert("users".into(), json_users.to_json());
        Json::Object(obj)
    }
}

pub trait Action: DbStored {
    fn get_base_action(&self) -> &BaseAction;
}
impl Action for StatusAction {
    fn get_base_action(&self) -> &BaseAction {
        &self.action
    }
}
impl Action for AnnouncementAction {
    fn get_base_action(&self) -> &BaseAction {
        &self.action
    }
}
impl Action for PresenceAction {
    fn get_base_action(&self) -> &BaseAction {
        &self.action
    }
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TypedAction {
    Status(StatusAction),
    Announcement(AnnouncementAction),
    Presence(PresenceAction),
}
