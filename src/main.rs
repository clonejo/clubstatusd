
extern crate chrono;
extern crate clap;
extern crate config;
extern crate hyper;
extern crate libc;
extern crate regex;
extern crate route_recognizer;
extern crate rustc_serialize;
extern crate rusqlite;
extern crate urlparse;

mod model;
mod db;
mod api;

use clap::{App, Arg};
use config::types::{Config, SettingsList};
use std::path::Path;
use std::io::{stderr, Write};

fn main() {
    let arg_matches = App::new("clubstatusd")
        .author("clonejo <clonejo@shakik.de>")
        .about("Backend with HTTP API that keeps your hackerspace's status (open/closed, \
                announcements, presence)")
        .arg(
            Arg::with_name("CONFIG")
                .short("c")
                .long("config")
                .takes_value(true)
                .help("set config file to use"))
        .get_matches();

    let config_path = Path::new(arg_matches.value_of("CONFIG").unwrap_or("/etc/clubstatusd.conf"));
    let conf = match config::reader::from_file(config_path) {
        Ok(c) => c,
        Err(err) => {
            if arg_matches.is_present("CONFIG") || config_path.is_file() {
                writeln!(&mut stderr(), "Error reading config file: {}", err).unwrap();
                std::process::exit(1);
            }
            writeln!(&mut stderr(), "No config file, assuming default values.").unwrap();
            Config::new(SettingsList::new())
        }
    };

    let db_path_str = conf.lookup_str_or("database_path", "/var/local/clubstatusd/db.sqlite");
    match db::connect(db_path_str) {
        Ok(con) => {
            let password = match conf.lookup_str("password") {
                Some(s) => Some(s),
                None => {
                    writeln!(&mut stderr(),
                             "No password set, the whole API will be available unauthenticated.").unwrap();
                    None
                }
            };
            api::run(con, conf.lookup_str_or("listen", "localhost:8000"), password);
        },
        Err(err) => {
            writeln!(&mut stderr(),
                     "Could not open database (path: {}), error message:\n{:?}", db_path_str, err).unwrap();
            std::process::exit(1);
        }
    }
}
