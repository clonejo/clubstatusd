use std::fmt;

use chrono::Utc;
use rocket::request::FromParam;
use rocket::serde::de::{self, Visitor};
use rocket::serde::{Deserialize, Deserializer, Serialize};

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
    pub user: UserName,
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
    pub fn new(note: String, time: i64, user: UserName, status: Status) -> Self {
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
    pub user: UserName,
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

#[derive(Clone, Debug, Serialize)]
pub struct PresenceAction {
    #[serde(flatten)]
    pub action: BaseAction,
    pub users: Vec<PresentNamedUser>,
    /// Number of anonymous users. Since this can contain guesses, it's a float.
    pub anonymous_users: f32,
}

impl PresenceAction {
    pub fn new(note: String, users: Vec<PresentNamedUser>, anonymous_count: f32) -> Self {
        Self::new_with_time(note, Utc::now().timestamp(), users, anonymous_count)
    }

    pub fn new_with_time(
        note: String,
        time: i64,
        users: Vec<PresentNamedUser>,
        anonymous_users: f32,
    ) -> Self {
        PresenceAction {
            action: BaseAction::new_with_time(note, time),
            users,
            anonymous_users,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PresentNamedUser {
    pub user: UserName,
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

#[derive(Hash, PartialEq, Eq, Clone, Serialize, PartialOrd, Ord)]
pub struct UserName(String);
impl UserName {
    pub fn new(name: String) -> Self {
        UserName(name)
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
impl<'de> Deserialize<'de> for UserName {
    fn deserialize<D>(deserializer: D) -> Result<UserName, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(UserNameVisitor)
    }
}
impl fmt::Debug for UserName {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        fmt.write_str("UserName(")?;
        self.0.fmt(fmt)?;
        fmt.write_str(")")
    }
}
struct UserNameVisitor;
impl<'de> Visitor<'de> for UserNameVisitor {
    type Value = UserName;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Usernames must be UTF-8 encoded, and 1-15 bytes.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let user = String::from(value);
        if user.len() == 0 || user.len() > 15 {
            return Err(E::custom(format!(
                "Username '{}' is either empty or longer than 15 bytes.",
                user
            )));
        }
        Ok(UserName(user))
    }
}
