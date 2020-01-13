use super::api::{Edit, NewMessage};
use super::Message;
use crate::api::{parse_query, IdQuery};
use crate::channels::{fire, Channel, ChannelMember, Event};
use crate::csrf::authenticate;
use crate::error::AppError;
use crate::messages::Preview;
use crate::{api, database};
use hyper::{Body, Request};
use crate::spaces::SpaceMember;

async fn send(req: Request<Body>) -> api::AppResult {
    let session = authenticate(&req).await?;
    let NewMessage {
        message_id,
        channel_id,
        name,
        text,
        entities,
        in_game,
        is_action,
    } = api::parse_body(req).await?;
    let mut conn = database::get().await;
    let db = &mut *conn;
    let channel_member = ChannelMember::get(db, &session.user_id, &channel_id)
        .await
        .ok_or(AppError::Unauthenticated)?;
    let name = name.unwrap_or(channel_member.character_name);
    let message = Message::create(
        db,
        message_id.as_ref(),
        &channel_id,
        &session.user_id,
        &*name,
        &*text,
        &entities,
        in_game,
        is_action,
        channel_member.is_master,
        None,
    )
    .await?;
    let result = api::Return::new(&message).build();
    fire(&channel_id, Event::new_message(message));
    result
}

async fn edit(req: Request<Body>) -> api::AppResult {
    let session = authenticate(&req).await?;
    let Edit {
        message_id,
        name,
        text,
        entities,
        in_game,
        is_action,
    } = api::parse_body(req).await?;
    let mut conn = database::get().await;
    let db = &mut *conn;
    let message = Message::get(db, &message_id, Some(&session.user_id)).await?;
    ChannelMember::get(db, &session.user_id, &message.channel_id)
        .await
        .ok_or(AppError::Unauthenticated)?;
    if message.sender_id != session.user_id {
        return Err(AppError::Unauthenticated);
    }

    let text = text.as_ref().map(String::as_str);
    let name = name.as_ref().map(String::as_str);
    let message = Message::edit(db, name, &message_id, text, &entities, in_game, is_action).await?;
    let result = api::Return::new(&message).build();
    let channel_id = message.channel_id.clone();
    fire(&channel_id, Event::message_edited(channel_id, message));
    result
}

async fn query(req: Request<Body>) -> api::AppResult {
    let api::IdQuery { id } = api::parse_query(req.uri())?;
    let mut conn = database::get().await;
    let db = &mut *conn;
    let user_id = authenticate(&req).await.ok().map(|session| session.user_id);
    let message = Message::get(db, &id, user_id.as_ref()).await?;
    api::Return::new(&message).build()
}

async fn delete(req: Request<Body>) -> api::AppResult {
    let session = authenticate(&req).await?;
    let api::IdQuery { id } = api::parse_body(req).await?;
    let mut conn = database::get().await;
    let db = &mut *conn;
    let message = Message::get(db, &id, None).await?;
    let space_member = SpaceMember::get_by_channel(db, &session.id, &message.channel_id)
        .await?
        .ok_or(AppError::Unauthenticated)?;
    if message.sender_id != session.user_id && !space_member.is_admin {
        return Err(AppError::Unauthenticated);
    }
    Message::delete(db, &id).await?;
    let channel_id = message.channel_id.clone();
    let event = Event::message_deleted(channel_id, message.id.clone());
    fire(&message.channel_id, event);
    api::Return::new(true).build()
}

async fn send_preview(req: Request<Body>) -> api::AppResult {
    let session = authenticate(&req).await?;
    let preview: Preview = api::parse_body(req).await?;

    if preview.sender_id != session.user_id {
        log::warn!("The user {} attempts to forge preview message.", session.user_id);
        return Err(AppError::BadRequest(format!("You are forging message")));
    }

    let mut conn = database::get().await;
    let db = &mut *conn;
    let channel_id = preview.channel_id.clone();

    ChannelMember::get(db, &session.user_id, &channel_id)
        .await
        .ok_or(AppError::Unauthenticated)?;
    fire(&channel_id, Event::message_preview(channel_id.clone(), preview));
    api::Return::new(true).build()
}

async fn by_channel(req: Request<Body>) -> api::AppResult {
    let IdQuery { id } = parse_query(req.uri())?;

    let mut db = database::get().await;
    let db = &mut *db;

    let channel = Channel::get_by_id(db, &id).await?;
    let messages = Message::get_by_channel(db, &channel.id).await?;
    api::Return::new(&messages).build()
}

pub async fn router(req: Request<Body>, path: &str) -> api::AppResult {
    use hyper::Method;

    match (path, req.method().clone()) {
        ("/query", Method::GET) => query(req).await,
        ("/by_channel", Method::GET) => by_channel(req).await,
        ("/send", Method::POST) => send(req).await,
        ("/delete", Method::DELETE) => delete(req).await,
        ("/edit", Method::POST) => edit(req).await,
        ("/preview", Method::POST) => send_preview(req).await,
        _ => Err(AppError::missing()),
    }
}
