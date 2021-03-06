use std::any::Any;
use std::cmp::{min, Ordering};
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::Read;
use std::num::ParseIntError;
use std::str;
use std::str::FromStr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use chrono::{Datelike, TimeZone, Utc};
use hyper::header;
use hyper::method::Method;
use hyper::server::{Request, Response, Server};
use hyper::status::StatusCode;
use hyper::uri::RequestUri;
use route_recognizer::{Match, Params, Router};
use rustc_serialize::hex::ToHex;
use rustc_serialize::json::{Json, Object, ToJson};
use sodiumoxide::crypto::pwhash;
use sodiumoxide::crypto::pwhash::Salt;
use spaceapi::Status as SpaceapiStatus;
use urlparse;

use crate::db;
use crate::db::DbCon;
use crate::model::Status;
use crate::model::{
    json_to_object, parse_time_string, QueryActionType, RequestObject, TypedAction,
};

pub mod mqtt;

trait Handler: Sync + Send + Any {
    fn handle(
        &self,
        pr: ParsedRequest,
        res: Response,
        con: Arc<Mutex<DbCon>>,
        presence_tracker: Arc<Mutex<Sender<String>>>,
        mqtt: Arc<Mutex<Option<Sender<TypedAction>>>>,
    );
}

impl<F> Handler for F
where
    F: Send
        + Sync
        + Any
        + Fn(
            ParsedRequest,
            Response,
            Arc<Mutex<DbCon>>,
            Arc<Mutex<Sender<String>>>,
            Arc<Mutex<Option<Sender<TypedAction>>>>,
        ),
{
    fn handle(
        &self,
        pr: ParsedRequest,
        res: Response,
        con: Arc<Mutex<DbCon>>,
        presence_tracker: Arc<Mutex<Sender<String>>>,
        mqtt: Arc<Mutex<Option<Sender<TypedAction>>>>,
    ) {
        (*self)(pr, res, con, presence_tracker, mqtt);
    }
}

type GetParams<'a> = HashMap<String, Option<String>>;

struct ParsedRequest<'a, 'b: 'a> {
    req: Request<'a, 'b>,
    path_params: Params,
    get_params_str: Option<String>,
    authenticated: bool,
}

pub fn run(
    shared_con: Arc<Mutex<DbCon>>,
    listen: &str,
    password: Option<String>,
    cookie_salt: Salt,
    mqtt: Option<Sender<TypedAction>>,
    spaceapi_static: Option<SpaceapiStatus>,
) {
    let mqtt_arc = Arc::new(Mutex::new(mqtt.clone()));
    let presence_tracker = Arc::new(Mutex::new(db::presence::start_tracker(
        shared_con.clone(),
        mqtt.clone(),
    )));

    let pass_cookie = match password {
        Some(pw) => {
            let cookie = generate_cookie(&cookie_salt, pw.as_str());
            Some((pw, cookie))
        }
        None => None,
    };

    let mut router: Router<(Method, Box<dyn Handler>)> = Router::new();
    router.add("/api/versions", (Method::Get, Box::new(api_versions)));
    router.add("/api/v0", (Method::Put, Box::new(create_action)));
    router.add("/api/v0/:type", (Method::Get, Box::new(query)));
    router.add(
        "/api/v0/status/current",
        (Method::Get, Box::new(status_current)),
    );
    router.add(
        "/api/v0/announcement/current",
        (Method::Get, Box::new(announcement_current)),
    );
    if let Some(s) = spaceapi_static {
        router.add(
            "/spaceapi",
            (
                Method::Get,
                Box::new(
                    move |_pr: ParsedRequest,
                          res: Response,
                          shared_con: Arc<Mutex<DbCon>>,
                          _: Arc<Mutex<Sender<String>>>,
                          _: Arc<Mutex<Option<Sender<TypedAction>>>>| {
                        spaceapi(res, shared_con, s.clone())
                    },
                ),
            ),
        );
    }

    Server::http(listen)
        .unwrap()
        .handle(move |req: Request, mut res: Response| {
            let (match_result, get_params_string) = {
                let uri_str = match req.uri {
                    RequestUri::AbsolutePath(ref p) => p,
                    _ => panic!(),
                };
                let (path_str, get_params_str) = split_uri(uri_str);
                (router.recognize(path_str), get_params_str.map(String::from))
            };
            match match_result {
                Ok(Match {
                    handler: tup,
                    params,
                }) => {
                    let &(ref method, ref handler): &(Method, Box<dyn Handler>) = tup;
                    let authenticated = match pass_cookie {
                        None => true,
                        Some((ref pass_str, ref cookie)) => {
                            check_authentication(&req, pass_str, cookie)
                        }
                    };
                    if let Some((_, ref cookie)) = pass_cookie {
                        if authenticated {
                            set_auth_cookie(&mut res, cookie.as_str());
                        } else {
                            clear_auth_cookie(&mut res);
                        }
                    }
                    if *method == req.method {
                        let pr = ParsedRequest {
                            req,
                            path_params: params,
                            // would be nicer to just put a reference into req here, but idk how:
                            get_params_str: get_params_string,
                            authenticated,
                        };
                        handler.handle(
                            pr,
                            res,
                            shared_con.clone(),
                            presence_tracker.clone(),
                            mqtt_arc.clone(),
                        );
                    } else {
                        send_status(res, StatusCode::MethodNotAllowed);
                    }
                }
                Err(_) => send_status(res, StatusCode::NotFound),
            };
        })
        .unwrap();
}

/*
 * split uri into path and parameters
 */
fn split_uri(uri_str: &str) -> (&str, Option<&str>) {
    let mut split = uri_str.splitn(2, '?');
    (split.next().unwrap(), split.next())
}

fn public_api_strip(json: &mut Json) {
    let obj = json.as_object_mut().unwrap();
    obj.remove("id");
    obj.remove("note");
    obj.remove("user");
    let status = obj.get_mut("status").unwrap();
    if status.as_string().unwrap() == "private" {
        *status = Json::String(String::from("closed"));
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

#[allow(clippy::unneeded_field_pattern)]
fn check_authentication(req: &Request, password: &str, cookie_value: &str) -> bool {
    if let Some(cookies) = req.headers.get::<header::Cookie>() {
        let correct_cookie = format!("clubstatusd-password={}", cookie_value);
        if cookies.iter().any(|c| c == correct_cookie.as_str()) {
            return true;
        }
    }
    match req.headers.get::<header::Authorization<header::Basic>>() {
        Some(&header::Authorization(header::Basic {
            username: _,
            password: Some(ref tried_password),
        })) => tried_password == password,
        _ => false,
    }
}

fn set_auth_cookie(res: &mut Response, cookie: &str) {
    // cookie expires in 1 to 2 years
    let expiration_year = Utc::today().year() + 2;
    let expire_time = Utc
        .ymd(expiration_year, 1, 1)
        .and_hms(0, 0, 0)
        .format("%a, %m %b %Y %H:%M:%S GMT");
    res.headers_mut().set(header::SetCookie(vec![format!(
        "clubstatusd-password={}; Path=/; Expires={}",
        cookie, expire_time
    )]));
}

fn clear_auth_cookie(res: &mut Response) {
    set_auth_cookie(res, "");
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

fn send_status(mut res: Response, status: StatusCode) {
    let s = res.status_mut();
    *s = status;
}

fn send_unauthorized(mut res: Response) {
    {
        let headers = res.headers_mut();
        headers.set_raw("WWW-Authenticate", vec![b"Basic".to_vec()]);
    }
    send_status(res, StatusCode::Unauthorized);
}

fn send(mut res: Response, status: StatusCode, msg: &[u8]) {
    {
        let s = res.status_mut();
        *s = status;
    }
    res.send(msg).unwrap();
}

fn api_versions(
    _pr: ParsedRequest,
    mut res: Response,
    _shared_con: Arc<Mutex<DbCon>>,
    _: Arc<Mutex<Sender<String>>>,
    _: Arc<Mutex<Option<Sender<TypedAction>>>>,
) {
    let mut obj = Object::new();
    obj.insert("versions".into(), [0].to_json());
    {
        let headers = res.headers_mut();
        headers.set(header::AccessControlAllowOrigin::Any);
        headers.set(header::ContentType::json());
    }
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
}

/*
 * PUT
 */
fn create_action(
    mut pr: ParsedRequest,
    mut res: Response,
    shared_con: Arc<Mutex<DbCon>>,
    presence_tracker: Arc<Mutex<Sender<String>>>,
    mqtt: Arc<Mutex<Option<Sender<TypedAction>>>>,
) {
    if !pr.authenticated {
        send_unauthorized(res);
        return;
    }
    // parse at maximum 1k bytes
    let action_buf = &mut [0; 1024];
    let bytes_read = pr.req.read(action_buf).unwrap();
    let (action_buf, _) = action_buf.split_at(bytes_read);
    let action_str = str::from_utf8(action_buf).unwrap();
    match Json::from_str(action_str) {
        Err(_) => send_status(res, StatusCode::BadRequest),
        Ok(action_json) => {
            let now = Utc::now().timestamp();
            match json_to_object(action_json, now) {
                Ok(RequestObject::Action(mut action)) => {
                    let mut con = shared_con.lock().unwrap();
                    let transaction = con.transaction().unwrap();
                    match action.store(&transaction, &*mqtt.lock().unwrap()) {
                        Some(action_id) => {
                            {
                                let headers = res.headers_mut();
                                headers.set(header::ContentType::json());
                            }
                            let mut resp_str = format!("{}", action_id);
                            resp_str.push('\n');
                            res.send(resp_str.as_bytes()).unwrap();
                        }
                        None => send(res, StatusCode::BadRequest, b"bad request"),
                    }
                    transaction.commit().unwrap();
                }
                Ok(RequestObject::PresenceRequest(username)) => {
                    presence_tracker.lock().unwrap().send(username).unwrap();
                }
                Err(msg) => {
                    send(res, StatusCode::BadRequest, msg.as_bytes());
                }
            }
        }
    }
}

/*
 * GET
 */
fn status_current(
    pr: ParsedRequest,
    mut res: Response,
    shared_con: Arc<Mutex<DbCon>>,
    _: Arc<Mutex<Sender<String>>>,
    _: Arc<Mutex<Option<Sender<TypedAction>>>>,
) {
    let get_params = parse_get_params(pr.get_params_str);
    let public_api = get_params.contains_key("public");
    if !public_api && !pr.authenticated {
        send_unauthorized(res);
        return;
    }
    let mut obj = Object::new();
    let con = shared_con.lock().unwrap();
    if !public_api {
        obj.insert(
            "last".into(),
            db::status::get_last(&*con).unwrap().to_json(),
        );
    }

    let mut changed_action = if public_api {
        db::status::get_last_changed_public(&*con)
            .unwrap()
            .to_json()
    } else {
        db::status::get_last_changed(&*con).unwrap().to_json()
    };
    if public_api {
        public_api_strip(&mut changed_action);
    }
    obj.insert("changed".into(), changed_action);

    {
        let headers = res.headers_mut();
        headers.set(header::ContentType::json());
        if public_api {
            headers.set(header::AccessControlAllowOrigin::Any);
        }
    }
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
}

fn announcement_current(
    pr: ParsedRequest,
    mut res: Response,
    shared_con: Arc<Mutex<DbCon>>,
    _: Arc<Mutex<Sender<String>>>,
    _: Arc<Mutex<Option<Sender<TypedAction>>>>,
) {
    let get_params = parse_get_params(pr.get_params_str);
    let public_api = get_params.contains_key("public");
    if !public_api && !pr.authenticated {
        send_unauthorized(res);
        return;
    }

    let mut obj = Object::new();
    let con = shared_con.lock().unwrap();
    let actions = if public_api {
        db::announcements::get_current_public(&*con)
    } else {
        db::announcements::get_current(&*con)
    };
    obj.insert("actions".into(), actions.unwrap().to_json());

    {
        let headers = res.headers_mut();
        headers.set(header::ContentType::json());
        if public_api {
            headers.set(header::AccessControlAllowOrigin::Any);
        }
    }
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
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

fn query(
    pr: ParsedRequest,
    mut res: Response,
    shared_con: Arc<Mutex<DbCon>>,
    _: Arc<Mutex<Sender<String>>>,
    _: Arc<Mutex<Option<Sender<TypedAction>>>>,
) {
    if !pr.authenticated {
        send_unauthorized(res);
        return;
    }

    let type_ = match pr.path_params.find("type").unwrap() {
        "all" => QueryActionType::All,
        "status" => QueryActionType::Status,
        "announcement" => QueryActionType::Announcement,
        "presence" => QueryActionType::Presence,
        _ => {
            send(res, StatusCode::BadRequest, b"bad action type");
            return;
        }
    };

    let get_params = parse_get_params(pr.get_params_str);
    let id: RangeExpr<IdExpr> = match get_params.get("id") {
        None => RangeExpr::range(IdExpr::Int(0), IdExpr::Last),
        Some(&None) => RangeExpr::range(IdExpr::Int(0), IdExpr::Last),
        Some(&Some(ref s)) => match s.parse() {
            Ok(id) => id,
            Err(_) => {
                send(res, StatusCode::BadRequest, b"bad parameter: id");
                return;
            }
        },
    };
    let time: RangeExpr<i64> = match get_params.get("time") {
        None => RangeExpr::range(i64::min_value(), i64::max_value()),
        Some(&None) => RangeExpr::range(i64::min_value(), i64::max_value()),
        Some(&Some(ref s)) => match s.parse::<RangeExpr<String>>() {
            Ok(t) => {
                let now = Utc::now().timestamp();
                match t.map(|s| parse_time_string(&*s, now)) {
                    Ok(m) => m,
                    Err(_) => {
                        send(res, StatusCode::BadRequest, b"bad parameter: time");
                        return;
                    }
                }
            }
            Err(_) => {
                send(res, StatusCode::BadRequest, b"bad parameter: time");
                return;
            }
        },
    };
    let count = match get_params.get("count") {
        None => 20,
        Some(&None) => 20,
        Some(&Some(ref s)) => match s.parse() {
            Ok(i) => i,
            Err(_) => {
                send(res, StatusCode::BadRequest, b"bad parameter: count");
                return;
            }
        },
    };
    let count: u64 = min(count, 100);
    let count = if id.is_single() { 1 } else { count };
    let take = match get_params.get("take") {
        None => Take::Last,
        Some(&None) => Take::Last,
        Some(&Some(ref s)) => match &**s {
            "first" => Take::First,
            "last" => Take::Last,
            _ => {
                send(res, StatusCode::BadRequest, b"bad parameter: take");
                return;
            }
        },
    };

    //println!("type: {:?} id: {:?} time: {:?} count: {:?} take: {:?}", type_, id, time, count, take);

    let mut obj = Object::new();
    let mut con = shared_con.lock().unwrap();
    let actions = db::query(type_, id, time, count, take, &mut *con);
    obj.insert("actions".into(), actions.unwrap().to_json());

    {
        let headers = res.headers_mut();
        headers.set(header::ContentType::json());
    }
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
}

fn spaceapi(mut res: Response, shared_con: Arc<Mutex<DbCon>>, spaceapi_static: SpaceapiStatus) {
    let changed_action = {
        let con = shared_con.lock().unwrap();
        db::status::get_last_changed_public(&*con).unwrap()
    };

    let mut status = spaceapi_static.clone();
    status.state.open = Some(changed_action.status == Status::Public);
    status.state.lastchange = Some(changed_action.action.time.try_into().unwrap());

    {
        let headers = res.headers_mut();
        headers.set(header::ContentType::json());
        headers.set(header::AccessControlAllowOrigin::Any);
    }
    let mut resp_str = serde_json::to_string(&status).unwrap();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
}
