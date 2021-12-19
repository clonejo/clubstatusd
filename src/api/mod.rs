use std::any::Any;
use std::cmp::{min, Ordering};
use std::collections::HashMap;
use std::convert::TryInto;
use std::error::Error;
use std::fmt;
use std::io::Cursor;
use std::io::Read;
use std::net::ToSocketAddrs;
use std::num::ParseIntError;
use std::str;
use std::str::FromStr;
use std::sync::mpsc::{Sender, SyncSender};
use std::sync::{Arc, Mutex};

use chrono::{Datelike, TimeZone, Utc};
use cookie::Expiration;
use regex::Regex;
use rocket::config::Config;
use rocket::data::{self, Data, FromData, ToByteUnit};
use rocket::http::ContentType;
use rocket::http::{self, Header};
use rocket::http::{Cookie, CookieJar, SameSite};
use rocket::request::{self, FromRequest, Request};
use rocket::response::{self, content, Responder, Response};
use rocket::serde::de::{self, Visitor};
use rocket::serde::{Deserialize, Deserializer, Serialize};
use rocket::{Build, Rocket, State};
use rocket_basicauth::BasicAuth;
//use hyper::header;
//use hyper::method::Method;
//use hyper::server::{Request, Response, Server};
//use hyper::status::StatusCode;
//use hyper::uri::RequestUri;
use route_recognizer::{Match, Params, Router};
use rustc_serialize::hex::ToHex;
use rustc_serialize::json::{Json, Object, ToJson};
use sodiumoxide::crypto::pwhash;
use sodiumoxide::crypto::pwhash::Salt;
use spaceapi::Status as SpaceapiStatus;

use crate::db;
use crate::db::DbCon;
use crate::db::DbStored;
use crate::model::Status;
use crate::model::{
    json_to_object, parse_time_string, AnnouncementAction, AnnouncementMethod, BaseAction,
    QueryActionType, RequestObject, StatusAction, TypedAction,
};

pub mod mqtt;

//trait Handler: Sync + Send + Any {
//    fn handle(
//        &self,
//        pr: ParsedRequest,
//        res: Response,
//        con: Arc<Mutex<DbCon>>,
//        presence_tracker: Arc<Mutex<Sender<String>>>,
//        mqtt: Arc<Mutex<Option<Sender<TypedAction>>>>,
//    );
//}

//impl<F> Handler for F
//where
//    F: Send
//        + Sync
//        + Any
//        + Fn(
//            ParsedRequest,
//            Response,
//            Arc<Mutex<DbCon>>,
//            Arc<Mutex<Sender<String>>>,
//            Arc<Mutex<Option<Sender<TypedAction>>>>,
//        ),
//{
//    fn handle(
//        &self,
//        pr: ParsedRequest,
//        res: Response,
//        con: Arc<Mutex<DbCon>>,
//        presence_tracker: Arc<Mutex<Sender<String>>>,
//        mqtt: Arc<Mutex<Option<Sender<TypedAction>>>>,
//    ) {
//        (*self)(pr, res, con, presence_tracker, mqtt);
//    }
//}

type GetParams<'a> = HashMap<String, Option<String>>;

//struct ParsedRequest<'a, 'b: 'a> {
//    req: Request<'a, 'b>,
//    path_params: Params,
//    get_params_str: Option<String>,
//    authenticated: bool,
//}

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
                //query,
                status_current,
                status_current_public,
                //announcement_current
            ],
        );

    //let mut router: Router<(Method, Box<dyn Handler>)> = Router::new();
    //router.add("/api/versions", (Method::Get, Box::new(api_versions)));
    //router.add("/api/v0", (Method::Put, Box::new(create_action)));
    //router.add("/api/v0/:type", (Method::Get, Box::new(query)));
    //router.add(
    //    "/api/v0/status/current",
    //    (Method::Get, Box::new(status_current)),
    //);
    //router.add(
    //    "/api/v0/announcement/current",
    //    (Method::Get, Box::new(announcement_current)),
    //);
    if let Some(s) = spaceapi_static {
        rocket = rocket.manage(s).mount("/", routes![spaceapi_]);
    }

    //Server::http(listen)
    //    .unwrap()
    //    .handle(move |req: Request, mut res: Response| {
    //        let (match_result, get_params_string) = {
    //            let uri_str = match req.uri {
    //                RequestUri::AbsolutePath(ref p) => p,
    //                _ => panic!(),
    //            };
    //            let (path_str, get_params_str) = split_uri(uri_str);
    //            (router.recognize(path_str), get_params_str.map(String::from))
    //        };
    //        match match_result {
    //            Ok(Match {
    //                handler: tup,
    //                params,
    //            }) => {
    //                let &(ref method, ref handler): &(Method, Box<dyn Handler>) = tup;
    //                let authenticated = match pass_cookie {
    //                    None => true,
    //                    Some((ref pass_str, ref cookie)) => {
    //                        check_authentication(&req, pass_str, cookie)
    //                    }
    //                };
    //                if let Some((_, ref cookie)) = pass_cookie {
    //                    if authenticated {
    //                        set_auth_cookie(&mut res, cookie.as_str());
    //                    } else {
    //                        clear_auth_cookie(&mut res);
    //                    }
    //                }
    //                if *method == req.method {
    //                    let pr = ParsedRequest {
    //                        req,
    //                        path_params: params,
    //                        // would be nicer to just put a reference into req here, but idk how:
    //                        get_params_str: get_params_string,
    //                        authenticated,
    //                    };
    //                    handler.handle(
    //                        pr,
    //                        res,
    //                        shared_con.clone(),
    //                        presence_tracker.clone(),
    //                        mqtt_arc.clone(),
    //                    );
    //                } else {
    //                    send_status(res, StatusCode::MethodNotAllowed);
    //                }
    //            }
    //            Err(_) => send_status(res, StatusCode::NotFound),
    //        };
    //    })
    //    .unwrap();
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
                set_auth_cookie(&cookie_jar, auth_secrets.cookie.as_str());
                return request::Outcome::Success(Authenticated {});
            }
        }
        let auth = req.guard::<BasicAuth>().await;
        let basic_auth_password = match auth {
            request::Outcome::Success(ref a) => a.password.as_str(),
            _ => "",
        };
        if basic_auth_password == auth_secrets.password {
            set_auth_cookie(&cookie_jar, auth_secrets.cookie.as_str());
            return request::Outcome::Success(Authenticated {});
        } else {
            clear_auth_cookie(&cookie_jar);
            return request::Outcome::Failure((
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
    (&key[..]).to_hex()
}

fn set_auth_cookie(cookie_jar: &CookieJar, cookie: &str) {
    // cookie expires in 1 to 2 years
    let expiration_year = Utc::today().year() + 2;
    let expire_time = Utc.ymd(expiration_year, 1, 1).and_hms(0, 0, 0);
    let cookie = Cookie::parse(format!(
        "clubstatusd-password={}; Path=/; Expires={}",
        cookie, expire_time
    ))
    .unwrap();
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

fn parse_get_params<'a>(get_params_str: Option<String>) -> GetParams<'a> {
    let mut params = HashMap::new();
    if let Some(ref params_str) = get_params_str {
        for pair in params_str.split('&') {
            let mut split = pair.splitn(2, '=');
            let key = urlparse::unquote_plus(split.next().unwrap()).unwrap();
            let value = split.next().map(|s| urlparse::unquote_plus(s).unwrap());
            params.insert(key, value);
        }
    }
    params
}

//fn send_status(mut res: Response, status: StatusCode) {
//    let s = res.status_mut();
//    *s = status;
//}

//fn send_unauthorized(mut res: Response) {
//    {
//        let headers = res.headers_mut();
//        headers.set_raw("WWW-Authenticate", vec![b"Basic".to_vec()]);
//    }
//    send_status(res, StatusCode::Unauthorized);
//}

//fn send(mut res: Response, status: StatusCode, msg: &[u8]) {
//    {
//        let s = res.status_mut();
//        *s = status;
//    }
//    res.send(msg).unwrap();
//}

#[derive(Serialize)]
struct ApiVersions {
    versions: Vec<usize>,
}

#[get("/api/versions")]
fn api_versions<'a>() -> RestResponder<ApiVersions> {
    RestResponder::new(
        AuthRequired::Public,
        http::Status::Ok,
        ApiVersions { versions: vec![0] },
    )
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
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

    async fn from_data(req: &'r Request<'_>, data: Data<'r>) -> data::Outcome<'r, Self> {
        use rocket::outcome::Outcome::*;
        use ActionRequestError::*;

        // Ensure the content type is correct before opening the data.
        let json_content_type = ContentType::new("application", "json");
        if req.content_type() != Some(&json_content_type) {
            return Forward(data);
        }

        // Read the data into a string.
        let string = match data.open(1024.bytes()).into_string().await {
            Ok(string) if string.is_complete() => string.into_inner(),
            Ok(_) => return Failure((http::Status::PayloadTooLarge, TooLarge)),
            Err(e) => return Failure((http::Status::InternalServerError, Io(e))),
        };

        let request = match serde_json::from_str(string.as_str()) {
            Ok(j) => j,
            Err(e) => return Failure((http::Status::UnprocessableEntity, JsonError(e))),
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
struct AnnouncementRequest {
    user: UserName,
    note: Note,
    aid: Option<u64>,
    method: AnnouncementMethod,
    from: Time,
    to: Time,
    public: bool,
}
#[derive(Deserialize)]
struct PresenceRequest {
    user: UserName,
}
struct UserName(String);
impl<'de> Deserialize<'de> for UserName {
    fn deserialize<D>(deserializer: D) -> Result<UserName, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(UserNameVisitor)
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
        if note.len() > 15 {
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
            user: self.user.0.clone(),
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
        let now = Utc::now().timestamp();
        AnnouncementAction {
            action: BaseAction {
                id: None,
                note: self.note.0.clone(),
                time: now,
            },
            aid: self.aid,
            method: self.method,
            from: self.from.absolute(now),
            to: self.to.absolute(now),
            user: self.user.0.clone(),
            public: self.public,
        }
        .store(transaction, mqtt)
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
    presence_tracker: &State<SyncSender<String>>,
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
                AuthRequired::Required,
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
                        AuthRequired::Required,
                        http::Status::Ok,
                        CreateActionResponse::ActionCreated(action_id),
                    ))
                }
                None => Ok(RestResponder::new(
                    AuthRequired::Required,
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
                        AuthRequired::Required,
                        http::Status::Ok,
                        CreateActionResponse::ActionCreated(action_id),
                    ))
                }
                None => Ok(RestResponder::new(
                    AuthRequired::Required,
                    http::Status::InternalServerError,
                    CreateActionResponse::Error,
                )),
            }
        }
        ActionRequest::Presence(PresenceRequest { user: username }) => {
            presence_tracker.send(username.0).unwrap();
            Ok(RestResponder::new(
                AuthRequired::Required,
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
    RestResponder::new(AuthRequired::Required, http::Status::Ok, status_current)
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
    RestResponder::new(AuthRequired::Public, http::Status::Ok, status_current)
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

#[derive(PartialEq)]
enum AuthRequired {
    Required,
    Public,
}
impl Default for AuthRequired {
    fn default() -> AuthRequired {
        AuthRequired::Required
    }
}

/**
 * Responder for publicly accessible data, for which no authentication is needed.
 *
 * Can optionally set header `Access-Control-Allow-Origin: *`.
 */
struct RestResponder<J: Serialize> {
    auth_required: AuthRequired,
    status: http::Status,
    response: J,
}
impl<J: Serialize> RestResponder<J> {
    fn new(auth_required: AuthRequired, status: http::Status, response: J) -> Self {
        RestResponder {
            auth_required,
            status,
            response,
        }
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
        if self.auth_required == AuthRequired::Public {
            res.header(Header::new("Access-Control-Allow-Origin", "*"));
        }
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
        let error_string = self.error.to_string();
        let mut res = Response::build();
        res.status(http::Status::UnprocessableEntity)
            .sized_body(error_string.len(), Cursor::new(error_string));
        Ok(res.finalize())
    }
}

//fn announcement_current(
//    pr: ParsedRequest,
//    mut res: Response,
//    shared_con: Arc<Mutex<DbCon>>,
//    _: Arc<Mutex<Sender<String>>>,
//    _: Arc<Mutex<Option<Sender<TypedAction>>>>,
//) {
//    let get_params = parse_get_params(pr.get_params_str);
//    let public_api = get_params.contains_key("public");
//    if !public_api && !pr.authenticated {
//        send_unauthorized(res);
//        return;
//    }
//
//    let mut obj = Object::new();
//    let con = shared_con.lock().unwrap();
//    let actions = if public_api {
//        db::announcements::get_current_public(&*con)
//    } else {
//        db::announcements::get_current(&*con)
//    };
//    obj.insert("actions".into(), actions.unwrap().to_json());
//
//    {
//        let headers = res.headers_mut();
//        headers.set(header::ContentType::json());
//        if public_api {
//            headers.set(header::AccessControlAllowOrigin::Any);
//        }
//    }
//    let mut resp_str = obj.to_json().to_string();
//    resp_str.push('\n');
//    res.send(resp_str.as_bytes()).unwrap();
//}

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

    fn map<F, R, E>(&self, f: F) -> Result<RangeExpr<R>, E>
    where
        F: Fn(&T) -> Result<R, E>,
    {
        Ok(match self {
            RangeExpr::Single(ref first) => RangeExpr::Single(f(first)?),
            RangeExpr::Range(ref first, ref second) => RangeExpr::Range(f(first)?, f(second)?),
        })
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

#[derive(Debug, PartialEq)]
pub enum IdExpr {
    Int(u64),
    Last,
}

impl PartialOrd for IdExpr {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use crate::api::IdExpr::*;

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

//fn query(
//    pr: ParsedRequest,
//    mut res: Response,
//    shared_con: Arc<Mutex<DbCon>>,
//    _: Arc<Mutex<Sender<String>>>,
//    _: Arc<Mutex<Option<Sender<TypedAction>>>>,
//) {
//    if !pr.authenticated {
//        send_unauthorized(res);
//        return;
//    }
//
//    let type_ = match pr.path_params.find("type").unwrap() {
//        "all" => QueryActionType::All,
//        "status" => QueryActionType::Status,
//        "announcement" => QueryActionType::Announcement,
//        "presence" => QueryActionType::Presence,
//        _ => {
//            send(res, StatusCode::BadRequest, b"bad action type");
//            return;
//        }
//    };
//
//    let get_params = parse_get_params(pr.get_params_str);
//    let id: RangeExpr<IdExpr> = match get_params.get("id") {
//        None => RangeExpr::range(IdExpr::Int(0), IdExpr::Last),
//        Some(&None) => RangeExpr::range(IdExpr::Int(0), IdExpr::Last),
//        Some(&Some(ref s)) => match s.parse() {
//            Ok(id) => id,
//            Err(_) => {
//                send(res, StatusCode::BadRequest, b"bad parameter: id");
//                return;
//            }
//        },
//    };
//    let time: RangeExpr<i64> = match get_params.get("time") {
//        None => RangeExpr::range(i64::min_value(), i64::max_value()),
//        Some(&None) => RangeExpr::range(i64::min_value(), i64::max_value()),
//        Some(&Some(ref s)) => match s.parse::<RangeExpr<String>>() {
//            Ok(t) => {
//                let now = Utc::now().timestamp();
//                match t.map(|s| parse_time_string(&*s, now)) {
//                    Ok(m) => m,
//                    Err(_) => {
//                        send(res, StatusCode::BadRequest, b"bad parameter: time");
//                        return;
//                    }
//                }
//            }
//            Err(_) => {
//                send(res, StatusCode::BadRequest, b"bad parameter: time");
//                return;
//            }
//        },
//    };
//    let count = match get_params.get("count") {
//        None => 20,
//        Some(&None) => 20,
//        Some(&Some(ref s)) => match s.parse() {
//            Ok(i) => i,
//            Err(_) => {
//                send(res, StatusCode::BadRequest, b"bad parameter: count");
//                return;
//            }
//        },
//    };
//    let count: u64 = min(count, 100);
//    let count = if id.is_single() { 1 } else { count };
//    let take = match get_params.get("take") {
//        None => Take::Last,
//        Some(&None) => Take::Last,
//        Some(&Some(ref s)) => match &**s {
//            "first" => Take::First,
//            "last" => Take::Last,
//            _ => {
//                send(res, StatusCode::BadRequest, b"bad parameter: take");
//                return;
//            }
//        },
//    };
//
//    //println!("type: {:?} id: {:?} time: {:?} count: {:?} take: {:?}", type_, id, time, count, take);
//
//    let mut obj = Object::new();
//    let mut con = shared_con.lock().unwrap();
//    let actions = db::query(type_, id, time, count, take, &mut *con);
//    obj.insert("actions".into(), actions.unwrap().to_json());
//
//    {
//        let headers = res.headers_mut();
//        headers.set(header::ContentType::json());
//    }
//    let mut resp_str = obj.to_json().to_string();
//    resp_str.push('\n');
//    res.send(resp_str.as_bytes()).unwrap();
//}

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

    RestResponder::new(AuthRequired::Public, http::Status::Ok, status)
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
