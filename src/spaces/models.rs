use chrono::naive::NaiveDateTime;
use postgres_types::FromSql;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::{CreationError, DbError, FetchError, Querist};

#[derive(Debug, Serialize, Deserialize, FromSql)]
#[serde(rename_all = "camelCase")]
#[postgres(name = "spaces")]
pub struct Space {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub created: NaiveDateTime,
    pub modified: NaiveDateTime,
    pub owner_id: Uuid,
    pub is_public: bool,
    pub deleted: bool,
    pub password: String,
    pub language: String,
    pub default_dice_type: String,
}

impl Space {
    pub async fn create<T: Querist>(
        db: &mut T,
        name: &str,
        owner_id: &Uuid,
        password: Option<&str>,
    ) -> Result<Space, CreationError> {
        db.create(include_str!("sql/create.sql"), &[&name, owner_id, &password])
            .await
            .map(|row| row.get(0))
    }

    pub async fn delete<T: Querist>(db: &mut T, id: &Uuid) -> Result<(), DbError> {
        db.execute(include_str!("sql/delete.sql"), &[id]).await.map(|_| ())
    }

    async fn get<T: Querist>(db: &mut T, id: Option<&Uuid>, name: Option<&str>) -> Result<Space, FetchError> {
        use postgres_types::Type;
        let join_owner = false;
        db.fetch_typed(
            include_str!("sql/get.sql"),
            &[Type::UUID, Type::TEXT, Type::BOOL],
            &[&id, &name, &join_owner],
        )
        .await
        .map(|row| row.get(0))
    }

    pub async fn all<T: Querist>(db: &mut T) -> Result<Vec<Space>, DbError> {
        let rows = db.query(include_str!("sql/all.sql"), &[]).await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }

    pub async fn get_by_id<T: Querist>(db: &mut T, id: &Uuid) -> Result<Space, FetchError> {
        Space::get(db, Some(id), None).await
    }

    pub async fn get_by_name<T: Querist>(db: &mut T, name: &str) -> Result<Space, FetchError> {
        Space::get(db, None, Some(name)).await
    }

    pub async fn is_public<T: Querist>(db: &mut T, id: &Uuid) -> Result<bool, FetchError> {
        db.fetch(include_str!("sql/is_public.sql"), &[id])
            .await
            .map(|row| row.get(0))
    }
}

#[derive(Debug, Serialize, Deserialize, FromSql)]
#[serde(rename_all = "camelCase")]
#[postgres(name = "space_members")]
pub struct SpaceMember {
    pub user_id: Uuid,
    pub space_id: Uuid,
    pub is_master: bool,
    pub is_admin: bool,
    pub join_date: NaiveDateTime,
}

impl SpaceMember {
    pub async fn set_master<T: Querist>(
        db: &mut T,
        user_id: &Uuid,
        space_id: &Uuid,
        is_master: bool,
    ) -> Result<SpaceMember, FetchError> {
        SpaceMember::set(db, user_id, space_id, None, Some(is_master)).await
    }

    pub async fn set_admin<T: Querist>(
        db: &mut T,
        user_id: &Uuid,
        space_id: &Uuid,
        is_admin: bool,
    ) -> Result<SpaceMember, FetchError> {
        SpaceMember::set(db, user_id, space_id, Some(is_admin), None).await
    }

    async fn set<T: Querist>(
        db: &mut T,
        user_id: &Uuid,
        space_id: &Uuid,
        is_admin: Option<bool>,
        is_master: Option<bool>,
    ) -> Result<SpaceMember, FetchError> {
        db.fetch(
            include_str!("sql/set_space_member.sql"),
            &[&is_admin, &is_master, user_id, space_id],
        )
        .await
        .map(|row| row.get(0))
    }

    pub async fn remove_user<T: Querist>(db: &mut T, user_id: &Uuid, space_id: &Uuid) -> Result<u64, DbError> {
        db.execute(include_str!("sql/remove_user_from_space.sql"), &[user_id, space_id])
            .await
    }

    pub async fn add_owner<T: Querist>(
        db: &mut T,
        user_id: &Uuid,
        space_id: &Uuid,
    ) -> Result<SpaceMember, CreationError> {
        let row = db
            .create(include_str!("sql/add_user_to_space.sql"), &[user_id, space_id, &true])
            .await?;
        Ok(row.get(1))
    }

    pub async fn add_user<T: Querist>(
        db: &mut T,
        user_id: &Uuid,
        space_id: &Uuid,
    ) -> Result<SpaceMember, CreationError> {
        let row = db
            .create(include_str!("sql/add_user_to_space.sql"), &[user_id, space_id, &false])
            .await?;
        Ok(row.get(1))
    }

    pub async fn get<T: Querist>(db: &mut T, user_id: &Uuid, space_id: &Uuid) -> Option<SpaceMember> {
        db.fetch(include_str!("sql/get_space_member.sql"), &[user_id, space_id])
            .await
            .map(|row| row.get(0))
            .ok()
    }

    pub async fn get_by_space<T: Querist>(db: &mut T, space_id: &Uuid) -> Result<Vec<SpaceMember>, DbError> {
        let rows = db
            .query(include_str!("sql/get_members_by_spaces.sql"), &[space_id])
            .await?;
        Ok(rows.into_iter().map(|row| row.get(0)).collect())
    }
}

#[derive(Debug, Serialize, Deserialize, FromSql)]
#[serde(rename_all = "camelCase")]
#[postgres(name = "restrained_members")]
pub struct RestrainedMember {
    pub user_id: Uuid,
    pub space_id: Uuid,
    pub blocked: bool,
    pub muted: bool,
    pub restrained_date: NaiveDateTime,
    pub operator_id: Option<Uuid>,
}

impl RestrainedMember {}

#[tokio::test]
async fn space_test() {
    use crate::database::Client;
    use crate::users::User;
    let mut client = Client::new().await;
    let mut trans = client.transaction().await.unwrap();
    let db = &mut trans;
    let email = "test@mythal.net";
    let username = "test_user";
    let password = "no password";
    let nickname = "Test User";
    let space_name = "Pure Illusion";
    let user = User::create(db, email, username, nickname, password).await.unwrap();
    let space = Space::create(db, space_name, &user.id, None).await.unwrap();
    let space = Space::get_by_name(db, &space.name).await.unwrap();
    let space = Space::get_by_id(db, &space.id).await.unwrap();
    assert!(Space::is_public(db, &space.id).await.unwrap());
    let spaces = Space::all(db).await.unwrap();
    assert!(spaces.into_iter().find(|s| s.id == space.id).is_some());

    // members
    SpaceMember::add_owner(db, &user.id, &space.id).await.unwrap();
    SpaceMember::get(db, &user.id, &space.id).await.unwrap();
    SpaceMember::set_admin(db, &user.id, &space.id, true).await.unwrap();
    SpaceMember::set_master(db, &user.id, &space.id, true).await.unwrap();
    let mut members = SpaceMember::get_by_space(db, &space.id).await.unwrap();
    let member = members.pop().unwrap();
    assert_eq!(member.user_id, user.id);
    assert_eq!(member.space_id, space.id);
    SpaceMember::remove_user(db, &user.id, &space.id).await.unwrap();
    assert!(SpaceMember::get(db, &user.id, &space.id).await.is_none());

    // delete
    Space::delete(db, &space.id).await.unwrap();
}