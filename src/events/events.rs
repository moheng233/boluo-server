use crate::channels::models::Member;
use crate::channels::Channel;
use crate::error::CacheError;
use crate::events::context;
use crate::events::context::{get_event_map, get_heartbeat_map, SyncEvent};
use crate::events::preview::{Preview, PreviewPost};
use crate::messages::Message;
use crate::utils::timestamp;
use crate::{cache, database};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::spawn;
use uuid::Uuid;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EventQuery {
    pub mailbox: Uuid,
    pub mailbox_type: MailBoxType,
    /// timestamp
    pub after: i64,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MailBoxType {
    Channel,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "type")]
pub enum ClientEvent {
    #[serde(rename_all = "camelCase")]
    Preview { preview: PreviewPost },
    #[serde(rename_all = "camelCase")]
    Heartbeat,
}

#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventBody {
    #[serde(rename_all = "camelCase")]
    NewMessage {
        message: Box<Message>,
    },
    #[serde(rename_all = "camelCase")]
    MessageDeleted {
        message_id: Uuid,
    },
    #[serde(rename_all = "camelCase")]
    MessageEdited {
        message: Box<Message>,
    },
    #[serde(rename_all = "camelCase")]
    MessagePreview {
        preview: Box<Preview>,
    },
    ChannelDeleted,
    #[serde(rename_all = "camelCase")]
    ChannelEdited {
        channel: Channel,
    },
    #[serde(rename_all = "camelCase")]
    Members {
        members: Vec<Member>,
    },
    Initialized,
    #[serde(rename_all = "camelCase")]
    Heartbeat {
        user_id: Uuid,
    },
    #[serde(rename_all = "camelCase")]
    HeartbeatMap {
        heartbeat_map: HashMap<Uuid, i64>,
    },
}

#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub mailbox: Uuid,
    pub mailbox_type: MailBoxType,
    pub timestamp: i64,
    pub body: EventBody,
}

impl Event {
    pub fn initialized(mailbox: Uuid, mailbox_type: MailBoxType) -> Event {
        Event {
            mailbox,
            mailbox_type,
            timestamp: timestamp(),
            body: EventBody::Initialized,
        }
    }

    pub async fn push_heartbeat_map(channel_id: Uuid, heartbeat_map: HashMap<Uuid, i64>) {
        let event = SyncEvent::new(Event {
            mailbox: channel_id,
            mailbox_type: MailBoxType::Channel,
            timestamp: timestamp(),
            body: EventBody::HeartbeatMap { heartbeat_map },
        });
        Event::send(channel_id, Arc::new(event)).await;
    }

    pub fn new_message(message: Message) {
        let channel_id = message.channel_id;
        let message = Box::new(message);
        Event::fire(EventBody::NewMessage { message }, channel_id, MailBoxType::Channel)
    }

    pub fn message_deleted(channel_id: Uuid, message_id: Uuid) {
        Event::fire(
            EventBody::MessageDeleted { message_id },
            channel_id,
            MailBoxType::Channel,
        )
    }

    pub fn message_edited(message: Message) {
        let channel_id = message.channel_id;
        let message = Box::new(message);
        Event::fire(EventBody::MessageEdited { message }, channel_id, MailBoxType::Channel)
    }

    pub fn channel_deleted(channel_id: Uuid) {
        Event::fire(EventBody::ChannelDeleted, channel_id, MailBoxType::Channel)
    }

    pub fn message_preview(preview: Box<Preview>) {
        let mailbox = preview.mailbox;
        let mailbox_type = preview.mailbox_type;
        Event::fire(EventBody::MessagePreview { preview }, mailbox, mailbox_type);
    }

    pub async fn heartbeat(mailbox: Uuid, user_id: Uuid) {
        let now = timestamp();
        let map = get_heartbeat_map();
        let mut map = map.lock().await;
        if let Some(heartbeat_map) = map.get_mut(&mailbox) {
            heartbeat_map.insert(user_id, now);
        } else {
            let mut heartbeat_map = HashMap::new();
            heartbeat_map.remove(&user_id);
            heartbeat_map.insert(user_id, now);
            map.insert(mailbox, heartbeat_map);
        }
    }

    pub fn push_members(channel_id: Uuid) {
        spawn(async move {
            if let Err(e) = Event::fire_members(channel_id).await {
                log::warn!("Failed to fetch member list: {}", e);
            }
        });
    }

    pub fn channel_edited(channel: Channel) {
        let channel_id = channel.id;
        Event::fire(EventBody::ChannelEdited { channel }, channel_id, MailBoxType::Channel);
    }

    pub fn cache_key(mailbox: &Uuid) -> Vec<u8> {
        cache::make_key(b"mailbox", mailbox, b"events")
    }

    pub async fn get_from_cache(mailbox: &Uuid, after: i64) -> Result<Vec<String>, CacheError> {
        if let Some(events) = get_event_map().read().await.get(mailbox) {
            let events = events
                .iter()
                .skip_while(|event| event.event.timestamp < after)
                .map(|event| event.encoded.clone())
                .collect();
            Ok(events)
        } else {
            Ok(vec![])
        }
    }

    async fn send(mailbox: Uuid, event: Arc<SyncEvent>) {
        let broadcast_table = context::get_broadcast_table();
        let table = broadcast_table.read().await;
        if let Some(tx) = table.get(&mailbox) {
            tx.send(event).ok();
        }
    }

    async fn fire_members(channel_id: Uuid) -> Result<(), anyhow::Error> {
        let mut db = database::get().await?;
        let members = Member::get_by_channel(&mut *db, channel_id).await?;
        drop(db);
        let event = SyncEvent::new(Event {
            mailbox: channel_id,
            mailbox_type: MailBoxType::Channel,
            body: EventBody::Members { members },
            timestamp: timestamp(),
        });

        Event::send(channel_id, Arc::new(event)).await;
        Ok(())
    }

    async fn async_fire(body: EventBody, mailbox: Uuid, mailbox_type: MailBoxType) {
        let preview_id = match &body {
            EventBody::MessagePreview { preview } => Some((preview.id, preview.sender_id)),
            _ => None,
        };
        let event = Arc::new(SyncEvent::new(Event {
            mailbox,
            body,
            mailbox_type,
            timestamp: timestamp(),
        }));
        let mut event_map = get_event_map().write().await;
        if let Some(events) = event_map.get_mut(&mailbox) {
            if let Some((preview_id, sender_id)) = preview_id {
                if let Some((i, _)) = events
                    .iter()
                    .rev()
                    .enumerate()
                    .take(32)
                    .find(|(_, e)| match &e.event.body {
                        EventBody::MessagePreview { preview } => {
                            preview.id == preview_id && preview.sender_id == sender_id
                        }
                        _ => false,
                    })
                {
                    let index = events.len() - 1 - i;
                    events[index] = event.clone();
                } else {
                    events.push_back(event.clone());
                }
            } else {
                events.push_back(event.clone());
            }
        } else {
            let mut events = VecDeque::new();
            events.push_back(event.clone());
            event_map.insert(mailbox, events);
        }

        Event::send(mailbox, event).await;
    }

    pub fn fire(body: EventBody, mailbox: Uuid, mailbox_type: MailBoxType) {
        spawn(Event::async_fire(body, mailbox, mailbox_type));
    }
}
