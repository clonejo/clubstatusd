use std::borrow::Cow;
use std::sync::mpsc::{sync_channel, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rumqtt::{MqttClient, MqttOptions, QoS, ReconnectOptions};

use crate::db::{status, DbCon};
use crate::model::*;

fn publish_status(action: &StatusAction, mqtt_client: &mut MqttClient, topic_prefix: &str) {
    {
        let payload = match action.status {
            Status::Public => "public",
            Status::Private => "private",
            Status::Closed => "closed",
        };
        mqtt_client
            .publish(
                format!("{}status", topic_prefix).as_str(),
                QoS::AtLeastOnce,
                true,
                payload,
            )
            .unwrap();
    }
    {
        mqtt_client
            .publish(
                format!("{}status/last", topic_prefix).as_str(),
                QoS::AtLeastOnce,
                true,
                serde_json::to_string(action).unwrap().as_bytes(),
            )
            .unwrap();
    }
}

fn publish_announcement(
    action: &AnnouncementAction,
    mqtt_client: &mut MqttClient,
    topic_prefix: &str,
) {
    mqtt_client
        .publish(
            format!("{}announcement/{}", topic_prefix, action.aid.unwrap()).as_str(),
            QoS::AtLeastOnce,
            false,
            serde_json::to_string(action).unwrap().as_bytes(),
        )
        .unwrap();
}

fn publish_presence<'a>(action: &'a PresenceAction, client: &mut MqttClient, topic_prefix: &str) {
    let mut users: Vec<Cow<str>> = action
        .users
        .iter()
        .filter_map(|u: &PresentNamedUser| {
            if u.status == PresentUserStatus::Left {
                return None;
            }
            Some(Cow::from(u.name.as_str()))
        })
        .collect();
    users.sort_unstable();
    users.push(Cow::from(format!(
        "{:.1} anonyme hackende",
        action.anonymous_users
    )));
    let users_string: String = users.join(", ");
    client
        .publish(
            format!("{}presence/list", topic_prefix).as_str(),
            QoS::AtLeastOnce,
            true,
            users_string,
        )
        .unwrap();
    for user in action.users.iter() {
        let status_str = match user.status {
            PresentUserStatus::Joined => "joined",
            PresentUserStatus::Present => "present",
            PresentUserStatus::Left => "left",
        };
        let name = user.name.as_str();
        client
            .publish(
                format!("{}presence/{}/{}", topic_prefix, status_str, name).as_str(),
                QoS::ExactlyOnce,
                false,
                user.since.to_string(),
            )
            .unwrap();
    }
    client
        .publish(
            format!("{}presence/present-anonymous", topic_prefix,).as_str(),
            QoS::ExactlyOnce,
            false,
            format!("{:.1}", action.anonymous_users),
        )
        .unwrap();
}

pub fn start_handler(
    server: Option<String>,
    port: u16,
    topic_prefix: String,
    shared_con: Arc<Mutex<DbCon>>,
) -> Option<SyncSender<TypedAction>> {
    let (tx, rx) = sync_channel::<TypedAction>(10);
    match server {
        Some(server_str) => {
            thread::Builder::new()
                .name(String::from("mqtt_client"))
                .spawn(move || {
                    println!("will connect to mqtt server {}, port {}", server_str, port);
                    let opts = MqttOptions::new("clubstatusd", server_str, port)
                        .set_keep_alive(30)
                        .set_reconnect_opts(ReconnectOptions::AfterFirstSuccess(30));
                    let (mut mqtt_client, _mqtt_receiver) = match MqttClient::start(opts) {
                        Ok(r) => r,
                        Err(e) => {
                            eprintln!("could not connect to mqtt server: {}", e);
                            std::process::exit(1);
                        }
                    };

                    let last_status = {
                        let con = shared_con.lock().unwrap();
                        status::get_last(&*con).unwrap()
                    };
                    publish_status(&last_status, &mut mqtt_client, &*topic_prefix);
                    println!("published current status on mqtt");

                    loop {
                        loop {
                            match rx.try_recv() {
                                Ok(msg) => match msg {
                                    TypedAction::Status(action) => {
                                        publish_status(&action, &mut mqtt_client, &*topic_prefix);
                                    }
                                    TypedAction::Announcement(action) => {
                                        publish_announcement(
                                            &action,
                                            &mut mqtt_client,
                                            &*topic_prefix,
                                        );
                                    }
                                    TypedAction::Presence(action) => {
                                        publish_presence(&action, &mut mqtt_client, &*topic_prefix);
                                    }
                                },
                                Err(TryRecvError::Empty) => {
                                    break;
                                }
                                Err(TryRecvError::Disconnected) => {
                                    return;
                                }
                            }
                        }
                        thread::sleep(Duration::new(2, 0));
                    }
                })
                .unwrap();
            Some(tx)
        }
        None => None,
    }
}
