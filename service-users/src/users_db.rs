use std::str::FromStr;

use anyhow::Result;
use deadpool_postgres::Object;
use time::format_description::well_known::Iso8601;
use tokio_postgres::types::Timestamp;
use uuid::Uuid;

use crate::proto::{Profile, User};

pub struct Token {
    pub id: Uuid,
    pub created: time::OffsetDateTime,
    pub user_id: Uuid,
}
impl TryFrom<tokio_postgres::Row> for Token {
    type Error = anyhow::Error;
    fn try_from(value: tokio_postgres::Row) -> std::result::Result<Self, Self::Error> {
        Ok(Token {
            id: value.try_get("id")?,
            created: value.try_get("created")?,
            user_id: value.try_get("user_id")?,
        })
    }
}

impl TryFrom<tokio_postgres::Row> for User {
    type Error = anyhow::Error;

    fn try_from(value: tokio_postgres::Row) -> std::result::Result<Self, Self::Error> {
        let id: Uuid = value.try_get("id")?;
        let created: time::OffsetDateTime = value.try_get("created")?;
        let created: String = created.format(&Iso8601::DEFAULT)?.to_string();
        let updated: time::OffsetDateTime = value.try_get("updated")?;
        let updated: String = updated.format(&Iso8601::DEFAULT)?.to_string();
        let deleted: Timestamp<time::OffsetDateTime> = value.try_get("deleted")?;
        let deleted: String = match deleted {
            Timestamp::PosInfinity => "infinity".to_string(),
            Timestamp::NegInfinity => "-infinity".to_string(),
            Timestamp::Value(date) => date.format(&Iso8601::DEFAULT)?.to_string(),
        };

        let email: String = value.try_get("email")?;
        let sub: String = value.try_get("sub")?;
        let role: i32 = value.try_get("role")?;
        let subscription_id: String = value.try_get("subscription_id")?;
        let subscription_end: Timestamp<time::OffsetDateTime> =
            value.try_get("subscription_end")?;
        let mut subscription_active: bool = false;
        let subscription_end: String = match subscription_end {
            Timestamp::PosInfinity => "infinity".to_string(),
            Timestamp::NegInfinity => "-infinity".to_string(),
            Timestamp::Value(date) => {
                if date < time::OffsetDateTime::now_utc() + time::Duration::days(2) {
                    subscription_active = true;
                }
                date.format(&Iso8601::DEFAULT)?.to_string()
            }
        };
        let subscription_check: Timestamp<time::OffsetDateTime> =
            value.try_get("subscription_check")?;
        let subscription_check: String = match subscription_check {
            Timestamp::PosInfinity => "infinity".to_string(),
            Timestamp::NegInfinity => "-infinity".to_string(),
            Timestamp::Value(date) => date.format(&Iso8601::DEFAULT)?.to_string(),
        };

        Ok(User {
            id: id.to_string(),
            created,
            updated,
            deleted,
            email,
            sub,
            role,
            subscription_id,
            subscription_end,
            subscription_check,
            subscription_active,
        })
    }
}

impl TryFrom<tokio_postgres::Row> for Profile {
    type Error = anyhow::Error;

    fn try_from(value: tokio_postgres::Row) -> std::result::Result<Self, Self::Error> {
        let id: Uuid = value.try_get("id")?;
        let created: time::OffsetDateTime = value.try_get("created")?;
        let created: String = created.format(&Iso8601::DEFAULT)?.to_string();
        let updated: time::OffsetDateTime = value.try_get("updated")?;
        let updated: String = updated.format(&Iso8601::DEFAULT)?.to_string();
        let deleted: Timestamp<time::OffsetDateTime> = value.try_get("deleted")?;
        let deleted: String = match deleted {
            Timestamp::PosInfinity => "infinity".to_string(),
            Timestamp::NegInfinity => "-infinity".to_string(),
            Timestamp::Value(date) => date.format(&Iso8601::DEFAULT)?.to_string(),
        };

        let user_id: Uuid = value.try_get("user_id")?;
        let user_id: String = user_id.to_string();
        let name: String = value.try_get("name")?;
        let about: String = value.try_get("about")?;
        let avatar_id: String = value.try_get("avatar_id")?;
        let avatar_url: String = value.try_get("avatar_url")?;
        let cover_id: String = value.try_get("cover_id")?;
        let cover_url: String = value.try_get("cover_url")?;
        let resume_id: String = value.try_get("resume_id")?;

        Ok(Profile {
            id: id.to_string(),
            created,
            updated,
            deleted,
            user_id,
            name,
            about,
            avatar_id,
            avatar_url,
            cover_id,
            cover_url,
            resume_id,
        })
    }
}

pub async fn select_token_by_id(conn: &Object, token_id: &str) -> Result<Token> {
    let token: tokio_postgres::Row = conn
        .query_one(
            "select * from tokens where id = $1",
            &[&Uuid::from_str(token_id)?],
        )
        .await?;
    let token: Token = Token::try_from(token)?;
    Ok(token)
}

pub async fn update_token_uuid(conn: &Object, user_id: &Uuid) -> Result<Uuid> {
    let token_id: Uuid = Uuid::now_v7();
    conn.execute(
        "update tokens set id = $1 where user_id = $2",
        &[&token_id, &user_id],
    )
    .await?;
    Ok(token_id)
}

pub async fn select_user_by_uuid(conn: &Object, user_uuid: Uuid) -> Result<User> {
    let user: tokio_postgres::Row = conn
        .query_one(
            "select * from users where id = $1 and deleted = 'infinity'",
            &[&user_uuid],
        )
        .await?;
    let user: User = User::try_from(user)?;
    Ok(user)
}

pub async fn select_profile_by_user_id(conn: &Object, user_id: &str) -> Result<Option<Profile>> {
    let user_id: Uuid = Uuid::from_str(user_id)?;
    let profile = conn
        .query_opt(
            "select * from profiles where user_id = $1 and deleted = 'infinity'",
            &[&user_id],
        )
        .await?;

    let profile = match profile {
        Some(profile) => {
            let profile: Profile = Profile::try_from(profile)?;
            Some(profile)
        }
        None => None,
    };
    Ok(profile)
}

pub async fn insert_profile(conn: &Object, user_id: &str, profile: &Profile) -> Result<Profile> {
    let user_id: Uuid = Uuid::from_str(user_id)?;
    let profile: tokio_postgres::Row = conn.query_one(
        "insert into profiles (id, user_id, name, about, avatar_id, avatar_url, cover_id, cover_url, resume_id) values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning *",
        &[&Uuid::now_v7(), &user_id, &profile.name, &profile.about, &profile.avatar_id, &profile.avatar_url, &profile.cover_id, &profile.cover_url, &profile.resume_id]
    ).await?;
    let profile: Profile = Profile::try_from(profile)?;
    Ok(profile)
}

pub async fn update_profile(conn: &Object, user_id: &str, profile: &Profile) -> Result<Profile> {
    let id = Uuid::from_str(&profile.id)?;
    let user_id: Uuid = Uuid::from_str(user_id)?;
    let profile: tokio_postgres::Row = conn.query_one(
        "update profiles set name = $1, about = $2, avatar_id = $3, avatar_url = $4, cover_id = $5, cover_url = $6, resume_id = $7 where id = $8 and user_id = $9 returning *",
        &[&profile.name, &profile.about, &profile.avatar_id, &profile.avatar_url, &profile.cover_id, &profile.cover_url, &profile.resume_id, &id, &user_id]
    ).await?;
    let profile: Profile = Profile::try_from(profile)?;
    Ok(profile)
}

