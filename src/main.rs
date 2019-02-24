
extern crate chrono;
extern crate clap;
extern crate config;
extern crate hyper;
extern crate libc;
extern crate regex;
extern crate route_recognizer;
extern crate rumqtt;
extern crate rusqlite;
extern crate rustc_serialize;
extern crate sodiumoxide;
extern crate time;
extern crate urlparse;

mod model;
mod db;
mod api;

mod model_tests;

use clap::{App, Arg};
use config::{Config, ConfigError};
use std::path::Path;
use std::io::{stderr, Write};
use std::sync::{Arc, Mutex};
use sodiumoxide::crypto::pwhash;
use sodiumoxide::crypto::pwhash::Salt;

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

    let config_path = Path::new(arg_matches.value_of("CONFIG").unwrap_or("/etc/clubstatusd"));
    let mut conf = Config::default();
    if let Err(err) =  conf.merge(config::File::with_name(config_path.to_str().unwrap())) {
        if arg_matches.is_present("CONFIG") || config_path.is_file() {
            writeln!(&mut stderr(), "Error reading config file: {}", err).unwrap();
            std::process::exit(1);
        }
        writeln!(&mut stderr(), "No config file, assuming default values.").unwrap();
    }

    let db_path_str = conf.get("database_path").unwrap_or("/var/local/clubstatusd/db.sqlite");
    match db::connect(db_path_str) {
        Ok(con) => {
            let password = match conf.get_str("password") {
                Ok(s) => Some(String::from(s)),
                Err(ConfigError::NotFound(_)) => {
                    writeln!(&mut stderr(),
                             "No password set, the whole API will be available unauthenticated.").unwrap();
                    None
                },
                Err(_) => {
                    panic!();
                }
            };
            let cookie_salt: Salt = match conf.get_str("cookie_salt") {
                Ok(s) => {
                    hex_str_to_salt(s.as_str())
                },
                Err(ConfigError::NotFound(_)) => {
                    pwhash::gen_salt()
                },
                Err(e) => {
                    dbg!(e);
                    panic!();
                }
            };

            let shared_con = Arc::new(Mutex::new(con));

            let mqtt_server = conf.get_str("mqtt.server").ok().map(|s| String::from(s));
            let mqtt_topic_prefix = conf.get("mqtt.topic_prefix").unwrap_or_else(|_| String::from(""));
            let mqtt_handler = api::mqtt::start_handler(mqtt_server, mqtt_topic_prefix,
                                                        shared_con.clone());

            let listen_addr = conf.get("listen").unwrap_or("localhost:8000");
            api::run(shared_con, listen_addr, password, cookie_salt, mqtt_handler);
        },
        Err(err) => {
            writeln!(&mut stderr(),
                     "Could not open database (path: {}), error message:\n{:?}", db_path_str, err).unwrap();
            std::process::exit(1);
        }
    }
}

fn hex_str_to_salt(s: &str) -> Salt {
    let mut bytes = Vec::new();
    for i in 0..32 {
        let nibbles = &s[2*i..2*i+2];
        let byte = u8::from_str_radix(nibbles, 16).unwrap();
        bytes.push(byte);
    }
    Salt::from_slice(bytes.as_slice()).unwrap()
}
