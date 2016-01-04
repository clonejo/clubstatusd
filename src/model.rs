
use chrono::*;

#[derive(Debug)]
pub struct BaseAction {
    pub id: Option<u64>,
    pub time: i64,
    pub note: String
}

#[derive(Debug)]
pub struct StatusAction {
    pub action: BaseAction,
    pub user: String,
    pub status: Status
}

#[derive(Debug)]
pub enum Status {
    Public,
    Private,
    Closed
}

impl StatusAction {
    pub fn new(note: String, user: String, status: Status) -> Self {
        StatusAction {
            action: BaseAction {
                id: Option::None,
                time: UTC::now().timestamp(),
                note: note
            },
            user: user,
            status: status
        }
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

trait BasicAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction;
}
impl BasicAction for StatusAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction { &self.action }
}
impl BasicAction for AnnouncementAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction { &self.action }
}
impl BasicAction for PresenceAction {
    fn get_base_action<'a>(&'a self) -> &'a BaseAction { &self.action }
}

