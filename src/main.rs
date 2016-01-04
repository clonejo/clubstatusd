
extern crate rusqlite;
extern crate chrono;
extern crate libc;

mod model;
mod db;

fn main() {
    let con = db::DbCon::new("db.sqlite");
    println!("db con: {:?}", con);
    println!("status::get_last: {:?}", db::status::get_last(&con));
    println!("announcements::get_last: {:?}", db::announcements::get_last(123, &con));
    println!("presence::get_last: {:?}", db::presence::get_last(&con));
    con.close();
}
