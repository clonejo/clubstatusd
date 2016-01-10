
use iron::{Iron, Request, Response, IronResult};
use iron::status;
use router::Router;
use db::DbCon;
use db;
use rustc_serialize::json::{Json, Object, ToJson};
use std::sync::{Arc, Mutex};
use std::io::Read;
use model::{json_to_object, Action};

pub fn run(con: DbCon) {
    let shared_con = Arc::new(Mutex::new(con));
    let mut router = Router::new();
    router.get("/api/versions", api_versions);

    let con_clone = shared_con.clone();
    router.put("/api/v0", move |req: &mut Request| {create_action(req, con_clone.clone())});
    let con_clone = shared_con.clone();
    router.get("/api/v0/status/current", move |req: &mut Request| {status_current(req, con_clone.clone())});
    Iron::new(router).http("localhost:8000").unwrap();
    //con.close();
}

fn api_versions(_req: &mut Request) -> IronResult<Response> {
    let mut obj = Object::new();
    obj.insert("versions".into(), [0].to_json());
    Ok(Response::with((status::Ok, obj.to_json().to_string())))
}

fn create_action(req: &mut Request, shared_con: Arc<Mutex<DbCon>>) -> IronResult<Response> {
    let mut action_string = String::new();
    req.body.read_to_string(&mut action_string).unwrap();
    let action_json = Json::from_str(&action_string).unwrap();
    let mut action = json_to_object(action_json);
    let con = shared_con.lock().unwrap();
    action.store(&*con);
    let mut resp_str = format!("{}", action.get_base_action().id.unwrap());
    resp_str.push('\n');
    Ok(Response::with((status::Ok, resp_str)))
}

fn status_current(_req: &mut Request, shared_con: Arc<Mutex<DbCon>>) -> IronResult<Response> {
    let mut obj = Object::new();
    let con = shared_con.lock().unwrap();
    obj.insert("last".into(), db::status::get_last(&*con).unwrap().to_json());
    obj.insert("changed".into(), db::status::get_last_changed(&*con).unwrap().to_json());
    let mut resp_str = obj.to_json().to_string();
    resp_str.push('\n');
    Ok(Response::with((status::Ok, resp_str)))
}
