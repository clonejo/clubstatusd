
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

use rumqtt::{MqttOptions, MqttClient, QoS, ReconnectOptions};

use model::*;
use db::{DbCon, status};

fn publish_status(action: &StatusAction, mqtt_client: &mut MqttClient, topic_prefix: &str) {
    use rustc_serialize::json::ToJson;

    {
        let payload = match action.status {
            Status::Public => "public",
            Status::Private => "private",
            Status::Closed => "closed"
        };
        mqtt_client.publish(format!("{}status", topic_prefix).as_str(), QoS::AtLeastOnce, true,
            payload).unwrap();
    }
    {
        mqtt_client.publish(format!("{}status/last", topic_prefix).as_str(), QoS::AtLeastOnce, true,
            action.to_json().to_string()).unwrap();
    }

}

fn publish_announcement(_action: &AnnouncementAction, mqtt_client: &mut MqttClient, topic_prefix: &str) {
        mqtt_client.publish(
            format!("{}announcement", topic_prefix).as_str(), QoS::AtLeastOnce, false,
            "not_implemented").unwrap();
}

fn publish_presence<'a>(action: &'a PresenceAction, client: &mut MqttClient, topic_prefix: &str) {
    let mut users: Vec<&'a str> = action.users.iter()
        .filter(|u: &&PresentUser| -> bool {
            u.status != PresentUserStatus::Left
        })
        .map(|u: &'a PresentUser| -> &'a str {
            &*u.name
        }).collect();
    users.sort();
    let users_string: String = users.join(",");
    client.publish(format!("{}presence/list", topic_prefix).as_str(), QoS::AtLeastOnce, true, users_string).unwrap();
    for user in action.users.iter() {
        let status_str = match user.status {
            PresentUserStatus::Joined  => "joined",
            PresentUserStatus::Present => "present",
            PresentUserStatus::Left    => "left"
        };
        client.publish(
            format!("{}presence/{}/{}", topic_prefix, status_str, user.name).as_str(), QoS::ExactlyOnce,
            false, user.since.to_string()).unwrap();
    }
}

pub fn start_handler(server: Option<String>, topic_prefix: String, shared_con: Arc<Mutex<DbCon>>)
    -> Option<Sender<TypedAction>> {

    let (tx, rx) = channel::<TypedAction>();
    match server {
        Some(server_str) => {
            thread::Builder::new().name(String::from("mqtt_client")).spawn(move || {
                let opts = MqttOptions::new("clubstatusd", server_str, 1883)
                    .set_keep_alive(30)
                    .set_reconnect_opts(ReconnectOptions::Always(30));
                let (mut mqtt_client, _mqtt_receiver) = MqttClient::start(opts)
                    .expect("could not connect to mqtt server");
                println!("connected to mqtt server");

                let last_status = {
                    let con = shared_con.lock().unwrap();
                    status::get_last(&*con).unwrap()
                };
                publish_status(&last_status, &mut mqtt_client, &*topic_prefix);

                loop {
                    loop {
                        match rx.try_recv() {
                            Ok(msg) => {
                                match msg {
                                    TypedAction::Status(action) => {
                                        publish_status(&action, &mut mqtt_client, &*topic_prefix);
                                    },
                                    TypedAction::Announcement(action) => {
                                        publish_announcement(&action, &mut mqtt_client, &*topic_prefix);
                                    },
                                    TypedAction::Presence(action) => {
                                        publish_presence(&action, &mut mqtt_client, &*topic_prefix);
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
