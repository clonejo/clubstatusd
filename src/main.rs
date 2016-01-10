
extern crate rusqlite;
extern crate chrono;
extern crate libc;
extern crate rustc_serialize;

extern crate iron;
extern crate router;

mod model;
mod db;
mod api;

fn main() {
    let con = db::connect("db.sqlite");
    api::run(con);
}
