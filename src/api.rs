
use std::any::Any;
use std::cmp::min;
use std::str;
use std::str::FromStr;
use std::collections::HashMap;
use std::num::ParseIntError;
use db::DbCon;
use db;
use rustc_serialize::json::{Json, Object, ToJson};
use std::sync::{Arc, Mutex};
use std::io::Read;
use std::sync::mpsc::Sender;
use model::{json_to_object, QueryActionType, RequestObject};
use hyper::server::{Server, Request, Response};
use hyper::header;
use hyper::uri::RequestUri;
use hyper::method::Method;
use hyper::status::StatusCode;
use route_recognizer::{Router, Match, Params};
use urlparse;
use chrono::UTC;

trait Handler: Sync + Send + Any {
    fn handle(&self, pr: ParsedRequest, res: Response, con: Arc<Mutex<DbCon>>,
              presence_tracker: Arc<Mutex<Sender<String>>>);
}

impl<F> Handler for F
where F: Send + Sync + Any + Fn(ParsedRequest, Response, Arc<Mutex<DbCon>>, Arc<Mutex<Sender<String>>>) {
    fn handle(&self, pr: ParsedRequest, res: Response, con: Arc<Mutex<DbCon>>,
              presence_tracker: Arc<Mutex<Sender<String>>>) {
        (*self)(pr, res, con, presence_tracker);
    }
}

type GetParams<'a> = HashMap<String, Option<String>>;

struct ParsedRequest<'a, 'b: 'a> {
    req: Request<'a, 'b>,
    path_params: Params,
    get_params_str: Option<String>,
    authenticated: bool
}

pub fn run(con: DbCon, listen: &str, password: Option<&str>) {
    let shared_con = Arc::new(Mutex::new(con));
    let password = password.map(|s| String::from(s));

    let presence_tracker = Arc::new(Mutex::new(db::presence::start_tracker(shared_con.clone())));

    let mut router: Router<(Method, Box<Handler>)> = Router::new();
    router.add("/api/versions", (Method::Get, Box::new(api_versions)));
    router.add("/api/v0", (Method::Put, Box::new(create_action)));
    router.add("/api/v0/:type", (Method::Get, Box::new(query)));
    router.add("/api/v0/status/current", (Method::Get, Box::new(status_current)));
    router.add("/api/v0/announcement/current", (Method::Get, Box::new(announcement_current)));

    Server::http(listen).unwrap().handle(move |req: Request, res: Response| {
        let (match_result, get_params_string) = {
            let uri_str = match req.uri {
                RequestUri::AbsolutePath(ref p) => p,
                _ => panic!()
            };
            let (path_str, get_params_str) = split_uri(uri_str);
            (router.recognize(path_str), get_params_str.map(|s| String::from(s)))
        };
        match match_result {
            Ok(Match{ handler: tup, params }) => {
                let &(ref method, ref handler): &(Method, Box<Handler>) = tup;
                let authenticated = check_authentication(&req, &password);
                if *method == req.method {
                    let pr = ParsedRequest {
                        req: req,
                        path_params: params,
                        // would be nicer to just put a reference into req here, but idk how:
                        get_params_str: get_params_string,
                        authenticated: authenticated
                    };
                    handler.handle(pr, res, shared_con.clone(), presence_tracker.clone());
                } else {
                    send_status(res, StatusCode::MethodNotAllowed);
                }
            },
            Err(_) =>
                send_status(res, StatusCode::NotFound)
        };
    }).unwrap();
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

fn check_authentication(req: &Request, password: &Option<String>) -> bool {
    match *password {
        None => true,
        Some(ref pass_str) => {
            match req.headers.get::<header::Authorization<header::Basic>>() {
                Some(&header::Authorization(header::Basic {
                    username: _,
                    password: Some(ref tried_password)
                })) => {
                    tried_password == pass_str
                },
                _ => false
            }
        }
    }
}

fn parse_get_params<'a>(get_params_str: Option<String>) -> GetParams<'a> {
    let mut params = HashMap::new();
    match get_params_str {
        Some(ref params_str) => {
            for pair in params_str.split('&') {
                let mut split = pair.splitn(2, '=');
                let key = urlparse::unquote_plus(split.next().unwrap()).unwrap();
                let value = split.next().map(|s| urlparse::unquote_plus(s).unwrap());
                params.insert(key, value);
            }
        },
        None => {}
    };
    params
}

fn send_status(mut res: Response, status: StatusCode) {
    let s = res.status_mut();
    *s = status;
}

fn send_unauthorized(mut res: Response) {
    {
        let headers = res.headers_mut();
        headers.set_raw("WWW-Authenticate", vec!["Basic".as_bytes().to_vec()]);
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

fn api_versions(_pr: ParsedRequest, mut res: Response, _shared_con: Arc<Mutex<DbCon>>,
                _: Arc<Mutex<Sender<String>>>) {
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
fn create_action(mut pr: ParsedRequest, mut res: Response, shared_con: Arc<Mutex<DbCon>>,
                 presence_tracker: Arc<Mutex<Sender<String>>>) {
    if !pr.authenticated {
        send_unauthorized(res);
        return;
    }
    let mut action_buf = &mut [0; 1024];
    // parse at maximum 1k bytes
    let bytes_read = pr.req.read(action_buf).unwrap();
    let (action_buf, _) = action_buf.split_at(bytes_read);
    let action_str = str::from_utf8(action_buf).unwrap();
    match Json::from_str(action_str) {
        Err(_) =>
            send_status(res, StatusCode::BadRequest),
        Ok(action_json) => {
            let now = UTC::now().timestamp();
            match json_to_object(action_json, now) {
                Ok(RequestObject::Action(mut action)) => {
                    let con = shared_con.lock().unwrap();
                    match action.store(&*con) {
                        Some(action_id) => {
                            {
                                let headers = res.headers_mut();
                                headers.set(header::ContentType::json());
                            }
                            let mut resp_str = format!("{}", action_id);
                            resp_str.push('\n');
                            res.send(resp_str.as_bytes()).unwrap();
                        },
                        None => {
                            send(res, StatusCode::BadRequest, "bad request".as_bytes())
                        }
                    }
                },
                Ok(RequestObject::PresenceRequest(username)) => {
                    presence_tracker.lock().unwrap().send(username).unwrap();
                },
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
fn status_current(pr: ParsedRequest, mut res: Response, shared_con: Arc<Mutex<DbCon>>,
                  _: Arc<Mutex<Sender<String>>>) {
    let get_params = parse_get_params(pr.get_params_str);
    let public_api = get_params.contains_key("public");
    if !public_api && !pr.authenticated {
        send_unauthorized(res);
        return;
    }
    let mut obj = Object::new();
    let con = shared_con.lock().unwrap();
    if !public_api {
        obj.insert("last".into(), db::status::get_last(&*con).unwrap().to_json());
    }

    let mut changed_action = if public_api {
        db::status::get_last_changed_public(&*con).unwrap().to_json()
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

fn announcement_current(pr: ParsedRequest, mut res: Response, shared_con: Arc<Mutex<DbCon>>,
                        _: Arc<Mutex<Sender<String>>>) {
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
    }
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
}


enum RangeExpr<T> {
    Single(T),
    Range(T, T)
}

impl<T: FromStr> FromStr for RangeExpr<T> {
    type Err = T::Err;

    fn from_str(s: &str) -> Result<Self, T::Err> {
        let mut parts = s.splitn(2, ':');
        let start = try!(parts.next().unwrap().parse());
        Ok(match parts.next() {
            None => {
                RangeExpr::Single(start)
            },
            Some(e) => {
                RangeExpr::Range(start, try!(e.parse()))
            }
        })
    }
}

enum IdExpr {
    Int(u64),
    Last
}

impl FromStr for IdExpr {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "last" => {
                Ok(IdExpr::Last)
            },
            _ => {
                s.parse()
            }
        }
    }
}

fn query(pr: ParsedRequest, mut res: Response, shared_con: Arc<Mutex<DbCon>>,
         _: Arc<Mutex<Sender<String>>>) {
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
            send(res, StatusCode::BadRequest, "bad action type".as_bytes());
            return;
        }
    };

    let get_params = parse_get_params(pr.get_params_str);
    let id: Option<RangeExpr<IdExpr>> = match get_params.get("id") {
        None => None,
        Some(&None) => None,
        Some(&Some(ref s)) => {
            match s.parse() {
                Ok(id) => Some(id),
                Err(_) => {
                    send(res, StatusCode::BadRequest, "bad parameter: id".as_bytes());
                    return;
                }
            }
        }
    };
    let count = match get_params.get("count") {
        None => 20,
        Some(&None) => 20,
        Some(&Some(ref s)) => {
            match s.parse() {
                Ok(i) => i,
                Err(_) => {
                    send(res, StatusCode::BadRequest, "bad parameter: count".as_bytes());
                    return;
                }
            }
        }
    };
    let count: u64 = min(count, 100);
    let id_is_single = id.map(|range| match range {
        RangeExpr::Single(_) => true,
        RangeExpr::Range(_, _) => false
    }).unwrap_or(false);
    let count = if id_is_single { 1 } else { count };

    //println!("type: {:?} count: {:?}", type_, count);

    let mut obj = Object::new();
    let con = shared_con.lock().unwrap();
    let actions = db::query(type_, count, &*con);
    obj.insert("actions".into(), actions.unwrap().to_json());

    {
        let headers = res.headers_mut();
        headers.set(header::ContentType::json());
    }
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    res.send(resp_str.as_bytes()).unwrap();
}
