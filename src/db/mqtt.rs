
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::thread;
use std::time::Duration;
use time;

use mqtt3::QoS;
use mqttc::{Client, ClientOptions, ReconnectMethod, PubOpt, PubSub};
use netopt::NetworkOptions;

use model::*;
use db::{DbCon, status};

fn publish_status(action: &StatusAction, client: &mut Client, topic_prefix: &str) {
    let payload = match action.status {
        Status::Public => "public",
        Status::Private => "private",
        Status::Closed => "closed"
    };
    let pubopt = PubOpt::new(QoS::AtLeastOnce, true);
    client.publish(format!("{}status", topic_prefix), payload, pubopt).unwrap();
}

fn publish_announcement(_action: &AnnouncementAction, client: &mut Client, topic_prefix: &str) {
        client.publish(
            format!("{}announcement", topic_prefix), "not_implemented",
            PubOpt::at_least_once()).unwrap();
}

fn publish_presence<'a>(action: &'a PresenceAction, client: &mut Client, topic_prefix: &str) {
    let mut users: Vec<&'a str> = action.users.iter()
        .filter(|u: &&PresentUser| -> bool {
            u.status != PresentUserStatus::Left
        })
        .map(|u: &'a PresentUser| -> &'a str {
            &*u.name
        }).collect();
    users.sort();
    let users_string: String = users.join(",");
    let pubopt = PubOpt::new(QoS::AtLeastOnce, true);
    client.publish(format!("{}presence/list", topic_prefix), users_string, pubopt).unwrap();
    for user in action.users.iter() {
        let status_str = match user.status {
            PresentUserStatus::Joined  => "joined",
            PresentUserStatus::Present => "present",
            PresentUserStatus::Left    => "left"
        };
        client.publish(
            format!("{}presence/{}/{}", topic_prefix, status_str, user.name), user.since.to_string(),
            PubOpt::at_most_once()).unwrap();
    }
}

pub fn start_handler(server: Option<String>, topic_prefix: String, shared_con: Arc<Mutex<DbCon>>)
    -> Option<Sender<TypedAction>> {

    let (tx, rx) = channel::<TypedAction>();
    match server {
        Some(server_str) => {
            thread::Builder::new().name(String::from("mqtt_client")).spawn(move || {
                let netopt = NetworkOptions::new();
                let mut opts = ClientOptions::new();
                opts.set_reconnect(ReconnectMethod::ReconnectAfter(Duration::from_secs(5)));
                let mut client =
                    opts.connect(&*server_str, netopt).expect("could not connect to mqtt server");
                println!("connected to mqtt server");

                let last_status = {
                    let con = shared_con.lock().unwrap();
                    status::get_last(&*con).unwrap()
                };
                publish_status(&last_status, &mut client, &*topic_prefix);

                let mut next_ping = time::precise_time_ns() + 30_000_000_000;
                loop {
                    loop {
                        match rx.try_recv() {
                            Ok(msg) => {
                                match msg {
                                    TypedAction::Status(action) => {
                                        publish_status(&action, &mut client, &*topic_prefix);
                                    },
                                    TypedAction::Announcement(action) => {
                                        publish_announcement(&action, &mut client, &*topic_prefix);
                                    },
                                    TypedAction::Presence(action) => {
                                        publish_presence(&action, &mut client, &*topic_prefix);
                                    }
                                }
                            },
                            Err(TryRecvError::Empty) => {
                                break;
                            },
                            Err(TryRecvError::Disconnected) => {
                                return;
                            }
                        }
                    }
                    if next_ping <= time::precise_time_ns() {
                        client.ping().unwrap();
                        next_ping = time::precise_time_ns() + 30_000_000_000;
                    }
                    thread::sleep(Duration::new(2, 0));
                }
            }).unwrap();
            Some(tx)
        },
        None => {
            None
        }
    }
}
