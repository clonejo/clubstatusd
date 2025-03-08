use serde::Serialize;
use url::Url;

use crate::{AnnouncementAction, AnnouncementMethod, BaseAction, Status, StatusAction};

#[derive(Debug, Serialize)]
pub struct PublicBaseAction {
    pub id: u64,
    pub time: i64,
    // no user
    // no note (PublicAnnouncementAction has a note field instead)
}
#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PublicStatus {
    Public,
    Closed,
}
#[derive(Debug, Serialize)]
pub struct PublicStatusAction {
    #[serde(flatten)]
    pub action: PublicBaseAction,
    pub status: PublicStatus,
}
#[derive(Debug, Serialize)]
pub struct PublicAnnouncementAction {
    #[serde(flatten)]
    pub action: PublicBaseAction,
    pub method: AnnouncementMethod,
    pub aid: u64, // announcement id
    pub from: i64,
    pub to: i64,

    pub note: String,
    pub url: Option<Url>,
}

pub trait ToPublic {
    type Public;
    fn to_public(&self) -> Self::Public;
}
impl ToPublic for Status {
    type Public = PublicStatus;
    fn to_public(&self) -> PublicStatus {
        match self {
            Status::Public => PublicStatus::Public,
            Status::Private => PublicStatus::Closed,
            Status::Closed => PublicStatus::Closed,
        }
    }
}
impl ToPublic for BaseAction {
    type Public = PublicBaseAction;
    fn to_public(&self) -> PublicBaseAction {
        PublicBaseAction {
            id: self.id.unwrap(),
            time: self.time,
        }
    }
}
impl ToPublic for StatusAction {
    type Public = PublicStatusAction;
    fn to_public(&self) -> PublicStatusAction {
        PublicStatusAction {
            action: self.action.to_public(),
            status: self.status.to_public(),
        }
    }
}
impl ToPublic for AnnouncementAction {
    type Public = PublicAnnouncementAction;
    fn to_public(&self) -> PublicAnnouncementAction {
        assert!(self.public);
        PublicAnnouncementAction {
            action: self.action.to_public(),
            method: self.method,
            aid: self.aid.unwrap(),
            from: self.from,
            to: self.to,
            note: self.action.note.clone(),
            url: self.url.clone(),
        }
    }
}
