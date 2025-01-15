use std::cmp::{min, Ordering};
use std::convert::TryInto;
use std::fmt;
use std::io::Cursor;
use std::net::ToSocketAddrs;
use std::num::ParseIntError;
use std::str;
use std::str::FromStr;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use chrono::{Datelike, TimeZone, Utc};
use cookie::Expiration;
use regex::Regex;
use rocket::data::{self, Data, FromData, ToByteUnit};
use rocket::fairing::AdHoc;
use rocket::form::{self, FromFormField, ValueField};
use rocket::http::{self, ContentType, Cookie, CookieJar, Header};
use rocket::request::{self, FromRequest, Request};
use rocket::response::{Responder, Response};
use rocket::serde::de::{self, Visitor};
use rocket::serde::{Deserialize, Deserializer, Serialize};
use rocket::{Build, Config, Rocket, State};
use rocket_basicauth::BasicAuth;
use sodiumoxide::crypto::pwhash;
use sodiumoxide::crypto::pwhash::Salt;
use spaceapi::Status as SpaceapiStatus;
use time::OffsetDateTime;

use crate::db;
use crate::db::DbCon;
use crate::db::DbStored;
use crate::model::Status;
use crate::model::{
    AnnouncementAction, AnnouncementMethod, BaseAction, QueryActionType, StatusAction, TypedAction,
    UserName,
};
use crate::util::bytes_to_hex;

mod ics;
pub mod mqtt;

pub fn run(
    shared_con: Arc<Mutex<DbCon>>,
    listen: &str,
    password: Option<String>,
    cookie_salt: Salt,
    mqtt: Option<SyncSender<TypedAction>>,
    spaceapi_static: Option<SpaceapiStatus>,
) -> Rocket<Build> {
    let presence_tracker = db::presence::start_tracker(shared_con.clone(), mqtt.as_ref());

    let auth_secrets = password.map(|p| AuthSecrets {
        cookie: generate_cookie(&cookie_salt, p.as_str()),
        password: p,
    });

    let mut config = Config::default();
    let socket_addr = listen.to_socket_addrs().unwrap().next().unwrap();
    config.address = socket_addr.ip();
    config.port = socket_addr.port();

    let mut rocket = rocket::custom(config)
        .manage(shared_con)
        .manage(auth_secrets)
        .manage(presence_tracker)
        .manage(mqtt)
        .register("/", catchers![unauthorized_catcher,])
        .mount(
            "/",
            routes![
                api_versions,
                create_action,
                query,
                status_current,
                status_current_public,
                announcement_current,
                announcement_current_public,
                ics::announcement_current,
                ics::announcement_current_public,
                all_options,
            ],
        )
        .attach(AdHoc::on_response("access-control", |_req, res| {
            Box::pin(async move {
                res.set_header(Header::new("Access-Control-Allow-Origin", "*"));
                res.set_header(Header::new("Access-Control-Allow-Headers", "Authorization"));
            })
        }));

    if let Some(s) = spaceapi_static {
        rocket = rocket.manage(s).mount("/", routes![spaceapi_]);
    }

    rocket
}

#[derive(Debug)]
struct AuthSecrets {
    password: String,
    cookie: String,
}
/**
 * Request guard, that checks if a user has provided the correct auth cookie, or has provided the
 * correct Basic Auth password, after which the cookie is set.
 *
 * If no password is configured in config, this does guard does nothing.
 */
struct Authenticated {
    // idea: add reference to request, so guard cannot be used without request
}
#[rocket::async_trait]
impl<'r> FromRequest<'r> for Authenticated {
    type Error = &'static str;
    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let cookie_jar = req.guard::<&CookieJar>().await.unwrap(); // CookieJar has Error=Infallible
        let auth_secrets = match &**req.guard::<&State<Option<AuthSecrets>>>().await.unwrap() {
            None => {
                // authentication is disabled
                return request::Outcome::Success(Authenticated {});
            }
            Some(s) => s,
        };
        if let Some(cookie) = cookie_jar.get("clubstatusd-password") {
            if cookie.value() == auth_secrets.cookie {
                // set cookie again to extend lifetime
                set_auth_cookie(cookie_jar, auth_secrets.cookie.as_str());
                return request::Outcome::Success(Authenticated {});
            }
        }
        let auth = req.guard::<BasicAuth>().await;
        let basic_auth_password = match auth {
            request::Outcome::Success(ref a) => a.password.as_str(),
            _ => "",
        };
        if basic_auth_password == auth_secrets.password {
            set_auth_cookie(cookie_jar, auth_secrets.cookie.as_str());
            return request::Outcome::Success(Authenticated {});
        } else {
            clear_auth_cookie(cookie_jar);
            return request::Outcome::Error((
                http::Status::Unauthorized,
                "Auth check failed. Please perform HTTP basic auth with the correct password.",
            ));
        }
    }
}

fn generate_cookie(cookie_salt: &Salt, password: &str) -> String {
    let mut key = vec![0; 32];
    pwhash::derive_key(
        key.as_mut_slice(),
        password.as_bytes(),
        cookie_salt,
        pwhash::OPSLIMIT_INTERACTIVE,
        pwhash::MEMLIMIT_INTERACTIVE,
    )
    .unwrap();
    bytes_to_hex(&key[..])
}

fn set_auth_cookie(cookie_jar: &CookieJar, cookie: &str) {
    // cookie expires in 1 to 2 years
    let expiration_year = Utc::now().year() + 2;
    let expire_time_chrono = Utc
        .with_ymd_and_hms(expiration_year, 1, 1, 0, 0, 0)
        .unwrap();
    let expire_time = OffsetDateTime::from_unix_timestamp(expire_time_chrono.timestamp()).unwrap();
    let cookie = Cookie::build(("clubstatusd-password", cookie.to_string()))
        .path("/")
        .expires(Expiration::DateTime(expire_time));
    cookie_jar.add(cookie);
}

fn clear_auth_cookie(cookie_jar: &CookieJar) {
    set_auth_cookie(cookie_jar, "");
}

#[catch(401)]
fn unauthorized_catcher<'r, 'o: 'r>() -> impl Responder<'r, 'o> {
    struct Resp {}
    impl<'r, 'o: 'r> Responder<'r, 'o> for Resp {
        fn respond_to(
            self,
            _request: &Request,
        ) -> Result<rocket::Response<'o>, rocket::http::Status> {
            let mut res = Response::build();
            res.header(Header::new("WWW-Authenticate", "Basic"));
            res.status(http::Status::Unauthorized);
            Ok(res.finalize())
        }
    }
    Resp {}
}

#[derive(Serialize)]
struct ApiVersions {
    versions: Vec<usize>,
}

#[get("/api/versions")]
fn api_versions() -> RestResponder<ApiVersions> {
    RestResponder::new(http::Status::Ok, ApiVersions { versions: vec![0] })
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ActionRequest {
    Status(StatusRequest),
    Announcement(AnnouncementRequest),
    Presence(PresenceRequest),
}
#[derive(Debug)]
enum ActionRequestError {
    TooLarge,
    Io(std::io::Error),
    JsonError(serde_json::Error),
}
#[rocket::async_trait]
impl<'r> FromData<'r> for ActionRequest {
    type Error = ActionRequestError;

    async fn from_data(_req: &'r Request<'_>, data: Data<'r>) -> data::Outcome<'r, Self> {
        use rocket::outcome::Outcome::*;
        use ActionRequestError::*;

        // Read the data into a string.
        let string = match data.open(1024.bytes()).into_string().await {
            Ok(string) if string.is_complete() => string.into_inner(),
            Ok(_) => return Error((http::Status::PayloadTooLarge, TooLarge)),
            Err(e) => return Error((http::Status::InternalServerError, Io(e))),
        };

        let request = match serde_json::from_str(string.as_str()) {
            Ok(j) => j,
            Err(e) => return Error((http::Status::UnprocessableEntity, JsonError(e))),
        };

        Success(request)
    }
}

#[derive(Deserialize)]
struct StatusRequest {
    user: UserName,
    status: Status,
    note: Note,
}
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
enum AnnouncementRequest {
    New {
        user: UserName,
        note: Note,
        from: Time,
        to: Time,
        public: bool,
    },
    Mod {
        aid: u64,
        user: UserName,
        note: Note,
        from: Time,
        to: Time,
        public: bool,
    },
    Del {
        aid: u64,
        user: UserName,
    },
    //FIXME: validate from <= to
    //FIXME: for method=new: validate now <= from
    //FIXME: for method=mod: validate:
    //  last action with same aid was method=new|mod and
    //  (now <= from or is unchanged if in the past)
}
#[derive(PartialEq, Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PresenceRequest {
    NamedUser {
        user: UserName,
    },
    AnonymousUsers {
        anonymous_client_id: u64,
        anonymous_users: f32,
    },
}
impl PartialOrd for PresenceRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        let s = match self {
            PresenceRequest::NamedUser { user } => (0, user.as_str(), 0),
            PresenceRequest::AnonymousUsers {
                anonymous_client_id,
                ..
            } => (1, "", *anonymous_client_id),
        };
        let o = match other {
            PresenceRequest::NamedUser { user } => (0, user.as_str(), 0),
            PresenceRequest::AnonymousUsers {
                anonymous_client_id,
                ..
            } => (1, "", *anonymous_client_id),
        };
        s.partial_cmp(&o)
    }
}
#[derive(Debug, PartialEq)]
struct Note(String);
impl<'de> Deserialize<'de> for Note {
    fn deserialize<D>(deserializer: D) -> Result<Note, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(NoteVisitor)
    }
}
struct NoteVisitor;
impl<'de> Visitor<'de> for NoteVisitor {
    type Value = Note;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Note must be UTF-8 encoded, and no longer than 80 bytes.")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        let note = String::from(value);
        if note.len() > 80 {
            return Err(E::custom(format!(
                "Note '{}' cannot be longer than 80 bytes.",
                note
            )));
        }
        Ok(Note(note))
    }
}
#[derive(Debug)]
pub enum Time {
    Timestamp(i64),
    Relative(i64),
}
impl Time {
    pub fn absolute(&self, now: i64) -> i64 {
        match self {
            Time::Timestamp(i) => *i,
            Time::Relative(i) => now + i,
        }
    }
}
impl<'de> Deserialize<'de> for Time {
    fn deserialize<D>(deserializer: D) -> Result<Time, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(TimeVisitor)
    }
}
struct TimeVisitor;
impl<'de> Visitor<'de> for TimeVisitor {
    type Value = Time;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("either integer (UNIX timestamp) or a string following the regex \"^now(?:([+-])(\\d+))?$\" (eg. \"now+3600\", meaning \"in 1 hour\")")
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Time::Timestamp(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_i64(
            value
                .try_into()
                .map_err(|_| E::custom("timestamp must fit into 64 bit signed int"))?,
        )
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_i64(value.round() as i64)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if let Ok(i) = value.parse::<i64>() {
            return self.visit_i64(i);
        }
        let re = Regex::new(r"^now(?:([+-])(\d+))?$").unwrap();
        match re.captures(value) {
            None => Err(E::custom("bad time specification")),
            Some(captures) => {
                let mut i: i64 = captures
                    .get(2)
                    .map(|c| c.as_str().parse::<u64>().unwrap() as i64)
                    .unwrap_or(0);
                match captures.get(1).map(|c| c.as_str()) {
                    Some("+") | None => {}
                    Some("-") => {
                        i = -i;
                    }
                    _ => {
                        panic!("should be impossible");
                    }
                }
                Ok(Time::Relative(i))
            }
        }
    }
}
impl DbStored for StatusRequest {
    fn store(
        &mut self,
        transaction: &rusqlite::Transaction<'_>,
        mqtt: Option<&SyncSender<TypedAction>>,
    ) -> Option<u64> {
        StatusAction {
            action: BaseAction {
                id: None,
                note: self.note.0.clone(),
                time: Utc::now().timestamp(),
            },
            status: self.status,
            user: self.user.clone(),
        }
        .store(transaction, mqtt)
    }
}
impl DbStored for AnnouncementRequest {
    fn store(
        &mut self,
        transaction: &rusqlite::Transaction<'_>,
        mqtt: Option<&SyncSender<TypedAction>>,
    ) -> Option<u64> {
        use AnnouncementRequest::*;

        let now = Utc::now().timestamp();
        let mut action = match self {
            New {
                note,
                from,
                to,
                user,
                public,
            } => AnnouncementAction {
                action: BaseAction {
                    id: None,
                    note: note.0.clone(),
                    time: now,
                },
                aid: None,
                method: AnnouncementMethod::New,
                from: from.absolute(now),
                to: to.absolute(now),
                user: user.clone(),
                public: *public,
            },
            Mod {
                aid,
                note,
                from,
                to,
                user,
                public,
            } => AnnouncementAction {
                action: BaseAction {
                    id: None,
                    note: note.0.clone(),
                    time: now,
                },
                aid: Some(*aid),
                method: AnnouncementMethod::Mod,
                from: from.absolute(now),
                to: to.absolute(now),
                user: user.clone(),
                public: *public,
            },
            Del { aid, user } => AnnouncementAction {
                // Most of the fields will just be ignored when stored.
                action: BaseAction {
                    id: None,
                    note: String::from(""),
                    time: now,
                },
                aid: Some(*aid),
                method: AnnouncementMethod::Del,
                from: 0,
                to: 0,
                user: user.clone(),
                public: false,
            },
        };
        action.store(transaction, mqtt)
    }
}

#[derive(Serialize)]
#[serde(untagged)]
enum CreateActionResponse {
    ActionCreated(u64),
    PresenceRecorded,
    Error,
}

// IDEA: add a new version of endpoint, which returns the full action that was created
#[put("/api/v0", data = "<action_request>")]
fn create_action(
    _authenticated: Authenticated,
    shared_con: &State<Arc<Mutex<DbCon>>>,
    presence_tracker: &State<SyncSender<PresenceRequest>>,
    mqtt: &State<Option<SyncSender<TypedAction>>>,
    action_request: Result<ActionRequest, ActionRequestError>,
) -> Result<RestResponder<CreateActionResponse>, JsonErrorResponder> {
    let action_request = match action_request {
        Ok(ar) => ar,
        Err(ActionRequestError::JsonError(err)) => {
            return Err(JsonErrorResponder::new(err));
        }
        Err(_) => {
            return Ok(RestResponder::new(
                http::Status::InternalServerError,
                CreateActionResponse::Error,
            ))
        }
    };
    match action_request {
        ActionRequest::Status(mut action) => {
            let mut con = shared_con.lock().unwrap();
            let transaction = con.transaction().unwrap();
            match action.store(&transaction, mqtt.as_ref()) {
                Some(action_id) => {
                    transaction.commit().unwrap();
                    Ok(RestResponder::new(
                        http::Status::Ok,
                        CreateActionResponse::ActionCreated(action_id),
                    ))
                }
                None => Ok(RestResponder::new(
                    http::Status::InternalServerError,
                    CreateActionResponse::Error,
                )),
            }
        }
        ActionRequest::Announcement(mut action) => {
            let mut con = shared_con.lock().unwrap();
            let transaction = con.transaction().unwrap();
            match action.store(&transaction, mqtt.as_ref()) {
                Some(action_id) => {
                    transaction.commit().unwrap();
                    Ok(RestResponder::new(
                        http::Status::Ok,
                        CreateActionResponse::ActionCreated(action_id),
                    ))
                }
                None => Ok(RestResponder::new(
                    http::Status::InternalServerError,
                    CreateActionResponse::Error,
                )),
            }
        }
        ActionRequest::Presence(presence_request) => {
            presence_tracker.send(presence_request).unwrap();
            Ok(RestResponder::new(
                http::Status::Ok,
                CreateActionResponse::PresenceRecorded,
            ))
        }
    }
}

#[get("/api/v0/status/current")]
fn status_current(
    _authenticated: Authenticated,
    shared_con: &State<Arc<Mutex<DbCon>>>,
) -> RestResponder<StatusCurrent> {
    let con = shared_con.lock().unwrap();
    let last = db::status::get_last(&*con).unwrap();
    let changed = db::status::get_last_changed(&*con).unwrap();
    let status_current = StatusCurrent { last, changed };
    RestResponder::new(http::Status::Ok, status_current)
}
#[get("/api/v0/status/current?public")]
fn status_current_public(
    shared_con: &State<Arc<Mutex<DbCon>>>,
) -> RestResponder<StatusCurrentPublic> {
    let con = shared_con.lock().unwrap();

    let changed = db::status::get_last_changed_public(&*con)
        .unwrap()
        .to_public();
    let status_current = StatusCurrentPublic { changed };
    RestResponder::new(http::Status::Ok, status_current)
}
#[derive(Serialize)]
struct StatusCurrent {
    last: StatusAction,
    changed: StatusAction,
}
#[derive(Serialize)]
struct StatusCurrentPublic {
    changed: PublicStatusAction,
}

/**
 * Responder for json.
 */
struct RestResponder<J: Serialize> {
    status: http::Status,
    response: J,
}
impl<J: Serialize> RestResponder<J> {
    fn new(status: http::Status, response: J) -> Self {
        RestResponder { status, response }
    }
}
impl<'r, 'o: 'r, J: Serialize> Responder<'r, 'o> for RestResponder<J> {
    fn respond_to(
        self,
        _req: &'r rocket::Request<'_>,
    ) -> Result<rocket::Response<'o>, rocket::http::Status> {
        let mut json_str = serde_json::to_string_pretty(&self.response).unwrap();
        json_str.push('\n'); // add trailing newline
        let mut res = Response::build();
        res.header(ContentType::JSON);
        res.status(self.status)
            .sized_body(json_str.len(), Cursor::new(json_str));
        Ok(res.finalize())
    }
}
struct JsonErrorResponder {
    error: serde_json::Error,
}
impl JsonErrorResponder {
    fn new(error: serde_json::Error) -> Self {
        JsonErrorResponder { error }
    }
}
impl<'r, 'o: 'r> Responder<'r, 'o> for JsonErrorResponder {
    fn respond_to(
        self,
        _req: &'r rocket::Request<'_>,
    ) -> Result<rocket::Response<'o>, rocket::http::Status> {
        let error_string = format!("{}\n", self.error);
        let mut res = Response::build();
        res.status(http::Status::UnprocessableEntity)
            .sized_body(error_string.len(), Cursor::new(error_string));
        Ok(res.finalize())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct CurrentAnnouncements {
    actions: Vec<AnnouncementAction>,
}
#[get("/api/v0/announcement/current")]
fn announcement_current(
    _authenticated: Authenticated,
    shared_con: &State<Arc<Mutex<DbCon>>>,
) -> RestResponder<CurrentAnnouncements> {
    let con = shared_con.lock().unwrap();
    let actions = db::announcements::get_current(&*con).unwrap();
    let r = CurrentAnnouncements { actions };
    RestResponder::new(http::Status::Ok, r)
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct CurrentPublicAnnouncements {
    actions: Vec<PublicAnnouncementAction>,
}
#[get("/api/v0/announcement/current?public")]
fn announcement_current_public(
    shared_con: &State<Arc<Mutex<DbCon>>>,
) -> RestResponder<CurrentPublicAnnouncements> {
    let con = shared_con.lock().unwrap();
    let actions = db::announcements::get_current_public(&*con)
        .unwrap()
        .iter()
        .map(|a| a.to_public())
        .collect();
    let r = CurrentPublicAnnouncements { actions };
    RestResponder::new(http::Status::Ok, r)
}

#[derive(Debug)]
pub enum RangeExpr<T> {
    Single(T),
    Range(T, T),
}

impl<T: PartialOrd> RangeExpr<T> {
    fn range(first: T, second: T) -> Self {
        if first == second {
            RangeExpr::Single(first)
        } else if first <= second {
            RangeExpr::Range(first, second)
        } else {
            RangeExpr::Range(second, first)
        }
    }

    fn is_single(&self) -> bool {
        match self {
            RangeExpr::Single(_) => true,
            RangeExpr::Range(_, _) => false,
        }
    }
}
impl<T: FromStr + PartialOrd> FromStr for RangeExpr<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, T::Err> {
        let mut parts = s.splitn(2, ':');
        let start = parts.next().unwrap().parse()?;
        Ok(match parts.next() {
            None => RangeExpr::Single(start),
            Some(e) => RangeExpr::range(start, e.parse()?),
        })
    }
}
#[rocket::async_trait]
impl<'r> FromFormField<'r> for RangeExpr<IdExpr> {
    fn from_value(field: ValueField<'r>) -> form::Result<'r, Self> {
        Ok(Self::from_str(field.value).unwrap())
    }
}
#[rocket::async_trait]
impl<'r> FromFormField<'r> for RangeExpr<i64> {
    fn from_value(field: ValueField<'r>) -> form::Result<'r, Self> {
        Ok(Self::from_str(field.value).unwrap())
    }
}

#[derive(Debug, PartialEq)]
pub enum IdExpr {
    Int(u64),
    Last,
}

impl PartialOrd for IdExpr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use IdExpr::*;

        match (self, other) {
            (&Int(i1), &Int(i2)) => i1.partial_cmp(&i2),
            (&Int(_), &Last) => Some(Ordering::Less),
            (&Last, &Int(_)) => Some(Ordering::Greater),
            (&Last, &Last) => Some(Ordering::Equal),
        }
    }
}

impl FromStr for IdExpr {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "last" => Ok(IdExpr::Last),
            _ => Ok(IdExpr::Int(s.parse().unwrap())),
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Take {
    First,
    Last,
}
#[rocket::async_trait]
impl<'r> FromFormField<'r> for Take {
    fn from_value(field: ValueField<'r>) -> form::Result<'r, Self> {
        match field.value {
            "first" => Ok(Take::First),
            "last" => Ok(Take::Last),
            _ => Err(form::Error::validation("take must be either 'first' or 'last'").into()),
        }
    }
    fn default() -> Option<Self> {
        Some(Take::Last)
    }
}

#[derive(FromForm)]
struct QueryParams {
    #[field(default = RangeExpr::range(IdExpr::Int(0), IdExpr::Last))]
    id: RangeExpr<IdExpr>,
    #[field(default = RangeExpr::range(i64::min_value(), i64::max_value()))]
    time: RangeExpr<i64>,
    #[field(default = 20)]
    count: u64,
    #[field(default = Take::Last)]
    take: Take,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
struct QueryResponse {
    actions: Vec<TypedAction>,
}

#[get("/api/v0/<type>?<params..>")]
fn query(
    _authenticated: Authenticated,
    shared_con: &State<Arc<Mutex<DbCon>>>,
    r#type: QueryActionType,
    params: QueryParams,
) -> RestResponder<QueryResponse> {
    let QueryParams {
        id,
        time,
        count,
        take,
    } = params;

    let count: u64 = min(count, 100);
    let count = if id.is_single() { 1 } else { count };

    let mut con = shared_con.lock().unwrap();
    let actions = db::query(r#type, id, time, count, take, &mut *con).unwrap();

    RestResponder::new(http::Status::Ok, QueryResponse { actions })
}

#[get("/spaceapi")]
fn spaceapi_(
    shared_con: &State<Arc<Mutex<DbCon>>>,
    spaceapi_static: &State<SpaceapiStatus>,
) -> RestResponder<SpaceapiStatus> {
    let changed_action = {
        let con = shared_con.lock().unwrap();
        db::status::get_last_changed_public(&*con).unwrap()
    };

    let mut status = spaceapi_static.inner().clone();
    status.state.open = Some(changed_action.status == Status::Public);
    status.state.lastchange = Some(changed_action.action.time.try_into().unwrap());

    RestResponder::new(http::Status::Ok, status)
}

#[derive(Debug, Serialize)]
struct PublicBaseAction {
    pub id: u64,
    pub time: i64,
    // no user
    // no note (PublicAnnouncementAction has a note field instead)
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum PublicStatus {
    Public,
    Closed,
}
#[derive(Debug, Serialize)]
struct PublicStatusAction {
    #[serde(flatten)]
    pub action: PublicBaseAction,
    pub status: PublicStatus,
}
#[derive(Debug, Serialize)]
struct PublicAnnouncementAction {
    #[serde(flatten)]
    pub action: PublicBaseAction,
    pub method: AnnouncementMethod,
    pub aid: u64, // announcement id
    pub from: i64,
    pub to: i64,

    pub note: String,
}

trait ToPublic {
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
        }
    }
}

/// Catches all OPTION requests in order to get the CORS related Fairing triggered.
#[options("/<_..>")]
fn all_options() {
    /* Intentionally left empty */
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::hex_str_to_salt;

    #[test]
    /// Make sure we do not inadvertantly log people out by changing the cookie generation
    /// algorithm.
    fn test_cookie_stable() {
        let salt =
            hex_str_to_salt("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff");
        assert_eq!(
            generate_cookie(&salt, "foo"),
            "600d731a45dd45bc98b002a2989442c41e73898017f7d4d5ba87d1942fbd3e60"
        );
    }

    #[test]
    fn test_note_deserialize() {
        assert_eq!(
            serde_json::from_str::<Note>(
                r#""👀 Dies ist eine Notiz, genau 80 UTF-8-Bytes lang.1234567890123456789012345678""#
            ).ok(),
            Some(Note(
                "👀 Dies ist eine Notiz, genau 80 UTF-8-Bytes lang.1234567890123456789012345678"
                    .to_string()
            ))
        );
        assert_eq!(
            serde_json::from_str::<Note>(
                r#""👀 Dies ist eine Notiz, weniger als 80 Zeichen, aber über 80 UTF-8-Bytes lang.""#
            ).ok(),
            None
        );
    }
}
