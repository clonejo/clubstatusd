#[macro_use]
extern crate rocket;

mod api;
mod db;
mod model;
mod util;

mod model_tests;

use std::sync::{Arc, Mutex};

use camino::Utf8PathBuf;
use clap::Parser;
use config::{Config, ConfigError};
use sodiumoxide::crypto::pwhash;
use sodiumoxide::crypto::pwhash::Salt;

#[derive(Parser)]
#[command()]
/// Backend with HTTP API that keeps your hackerspace's status (open/closed, announcements, presence)
struct Clubstatusd {
    #[arg(short, long, default_value_t=Utf8PathBuf::from("/etc/clubstatusd"), help="set config file to use")]
    config: Utf8PathBuf,
}

#[launch]
async fn rocket() -> _ {
    let args = Clubstatusd::parse();

    let config_path = args.config;
    let conf_builder = Config::builder().add_source(config::File::with_name(config_path.as_str()));
    let conf = match conf_builder.build() {
        Ok(conf) => conf,
        Err(err) => {
            if config_path.is_file() {
                eprintln!("Error reading config file: {}", err);
                std::process::exit(1);
            }
            eprintln!(
                "No config file found at {}, assuming default values.",
                config_path
            );
            Config::default()
        }
    };

    let db_path_str = conf
        .get_string("database_path")
        .unwrap_or_else(|_| String::from("/var/local/clubstatusd/db.sqlite"));
    let con = match db::connect(db_path_str.as_str()) {
        Ok(con) => con,
        Err(err) => {
            eprintln!(
                "Could not open database (path: {}), error message:\n{:?}",
                db_path_str, err
            );
            std::process::exit(1);
        }
    };
    let password = match conf.get_string("password") {
        Ok(s) => Some(s),
        Err(ConfigError::NotFound(_)) => {
            eprintln!("No password set, the whole API will be available unauthenticated.");
            None
        }
        Err(e) => {
            dbg!(e);
            panic!();
        }
    };
    let cookie_salt: Salt = match conf.get_string("cookie_salt") {
        Ok(s) => hex_str_to_salt(s.as_str()),
        Err(ConfigError::NotFound(_)) => pwhash::gen_salt(),
        Err(e) => {
            dbg!(e);
            panic!();
        }
    };

    let shared_con = Arc::new(Mutex::new(con));

    let mqtt_server = conf.get_string("mqtt.server").ok();
    let port = conf.get_int("mqtt.port").unwrap_or(1883) as u16;
    let mqtt_topic_prefix = conf
        .get_string("mqtt.topic_prefix")
        .unwrap_or_else(|_| String::from(""));
    let mqtt_handler =
        api::mqtt::start_handler(mqtt_server, port, mqtt_topic_prefix, shared_con.clone());

    let spaceapi_static = conf
        .get_string("spaceapi")
        .ok()
        .map(|s| serde_json::from_str(s.as_str()).expect("Cannot parse spaceapi json."));

    let listen_addr = conf
        .get_string("listen")
        .unwrap_or_else(|_| String::from("localhost:8000"));
    api::run(
        shared_con,
        listen_addr.as_str(),
        password,
        cookie_salt,
        mqtt_handler,
        spaceapi_static,
    )
}

fn hex_str_to_salt(s: &str) -> Salt {
    let mut bytes = Vec::new();
    for i in 0..32 {
        let nibbles = &s[2 * i..2 * i + 2];
        let byte = u8::from_str_radix(nibbles, 16).unwrap();
        bytes.push(byte);
    }
    Salt::from_slice(bytes.as_slice()).unwrap()
}
