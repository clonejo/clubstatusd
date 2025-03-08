use rocket::request::FromParam;

use crate::db::DbStored;
use clubstatus_types::*;

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
