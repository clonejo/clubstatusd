use std::io::Cursor;
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};
use icalendar::{Calendar, Component, Event};
use rocket::http::{self, ContentType, Header};
use rocket::response::{Responder, Response};
use rocket::State;
use uuid::Uuid;

use super::{AuthRequired, Authenticated, DbCon, ToPublic};

#[get("/api/v0/announcement/current.ics")]
pub(super) fn announcement_current(
    _authenticated: Authenticated,
    shared_con: &State<Arc<Mutex<DbCon>>>,
) -> IcsResponder {
    let con = shared_con.lock().unwrap();
    let actions = crate::db::announcements::get_current(&*con).unwrap();
    let ics: Calendar = actions
        .iter()
        .map(|a| {
            let mut ev = Event::new();
            event_set_uuid_from_aid(&mut ev, a.aid.unwrap());
            ev.summary(&a.action.note);
            ev.starts(Utc.timestamp(a.from, 0));
            ev.ends(Utc.timestamp(a.to, 0));
            ev
        })
        .collect();
    IcsResponder::new(AuthRequired::Required, http::Status::Ok, ics)
}

#[get("/api/v0/announcement/current.ics?public")]
pub(super) fn announcement_current_public(shared_con: &State<Arc<Mutex<DbCon>>>) -> IcsResponder {
    let con = shared_con.lock().unwrap();
    let actions = crate::db::announcements::get_current_public(&*con).unwrap();
    let public = actions.iter().map(|a| a.to_public());
    let ics: Calendar = public
        .map(|a| {
            let mut ev = Event::new();
            event_set_uuid_from_aid(&mut ev, a.aid);
            ev.summary(&a.note);
            ev.starts(Utc.timestamp(a.from, 0));
            ev.ends(Utc.timestamp(a.to, 0));
            ev
        })
        .collect();
    IcsResponder::new(AuthRequired::Public, http::Status::Ok, ics)
}

fn event_set_uuid_from_aid(event: &mut Event, aid: u64) {
    let namespace_uuid = Uuid::parse_str("6fda1deb-16f7-4901-a3cb-eb65069c0db9").unwrap();
    let aid_bytes = aid.to_le_bytes();
    event.uid(
        Uuid::new_v5(&namespace_uuid, &aid_bytes)
            .to_hyphenated()
            .encode_lower(&mut Uuid::encode_buffer()),
    );
}

pub(super) struct IcsResponder {
    auth_required: AuthRequired,
    status: http::Status,
    calendar: Calendar,
}
impl IcsResponder {
    // TODO: either take the Authenticated guard (with reference to request) or a new
    // Unauthenticated guard (also with reference to request) as paramater, to avoid mistakes with
    // `auth_required`.
    fn new(auth_required: AuthRequired, status: http::Status, calendar: Calendar) -> Self {
        IcsResponder {
            auth_required,
            status,
            calendar,
        }
    }
}
impl<'r, 'o: 'r> Responder<'r, 'o> for IcsResponder {
    fn respond_to(
        self,
        _req: &'r rocket::Request<'_>,
    ) -> Result<rocket::Response<'o>, rocket::http::Status> {
        let mut s = self.calendar.to_string();
        s.push('\n'); // add trailing newline
        let mut res = Response::build();
        res.header(ContentType::new("text", "calendar; charset=utf-8"));
        if self.auth_required == AuthRequired::Public {
            res.header(Header::new("Access-Control-Allow-Origin", "*"));
        }
        res.status(self.status).sized_body(s.len(), Cursor::new(s));
        Ok(res.finalize())
    }
}
