use crate::proto::users_service_server::UsersService;
use crate::proto::{AuthRequest, Empty, PaymentId, User, UserIds};
use crate::proto::{UserId, UserRole};
use crate::MyService;
use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::models::*;
use crate::schema::users::dsl::*;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;

impl TryFrom<DieselUser> for User {
    type Error = tonic::Status;

    fn try_from(user: DieselUser) -> Result<Self, Self::Error> {
        Ok(User {
            id: user.id,
            created: user.created.to_string(),
            updated: user.updated.to_string(),
            deleted: user.deleted.map(|d| d.to_string()),
            email: user.email,
            role: UserRole::from_str_name(&user.role)
                .unwrap_or(UserRole::RoleUser)
                .into(),
            sub: user.sub,
            name: user.name,
            avatar_id: user.avatar_id,
            payment_id: Some(user.payment_id),
        })
    }
}

#[tonic::async_trait]
impl UsersService for MyService {
    type GetUsersStream = ReceiverStream<Result<User, Status>>;

    async fn auth(&self, request: Request<AuthRequest>) -> Result<Response<User>, Status> {
        println!("Auth");
        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let request = request.into_inner();

        let user = diesel::update(users)
            .filter(email.eq(&request.email))
            .filter(sub.eq(&request.sub))
            .set(updated.eq(diesel::dsl::now))
            .get_result::<DieselUser>(&mut conn)
            .await;

        match user {
            Ok(row) => {
                if row.deleted.is_some() {
                    return Err(Status::unauthenticated("Unauthenticated"));
                }
                let user: User = row.try_into()?;
                println!("Elapsed: {:?}", start.elapsed());
                Ok(Response::new(user))
            }
            Err(_) => {
                let user = diesel::insert_into(users)
                    .values((
                        id.eq(Uuid::now_v7().as_bytes().to_vec()),
                        email.eq(&request.email),
                        role.eq(UserRole::as_str_name(&UserRole::RoleUser)),
                        sub.eq(&request.sub),
                    ))
                    .get_result::<DieselUser>(&mut conn)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
                let user: User = user.try_into()?;
                println!("Elapsed: {:?}", start.elapsed());
                Ok(Response::new(user))
            }
        }
    }

    async fn get_users(
        &self,
        request: Request<UserIds>,
    ) -> Result<Response<Self::GetUsersStream>, Status> {
        #[cfg(debug_assertions)]
        println!("GetUsers");
        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let user_ids = request.into_inner().user_ids;

        let results = users
            .filter(id.eq_any(&user_ids))
            .select(DieselUser::as_select())
            .load(&mut conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            for user in results {
                let user: User = match user.try_into() {
                    Ok(user) => user,
                    Err(e) => {
                        tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                        return;
                    }
                };
                tx.send(Ok(user)).await.unwrap();
            }
            println!("Elapsed: {:?}", start.elapsed());
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_user(&self, request: Request<UserId>) -> Result<Response<User>, Status> {
        #[cfg(debug_assertions)]
        println!("GetUser");

        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();

        let user: DieselUser = users
            .filter(id.eq(&request.user_id))
            .first(&mut conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let user: User = user.try_into()?;

        println!("Elapsed: {:?}", start.elapsed());
        Ok(Response::new(user))
    }

    async fn update_user(&self, request: Request<User>) -> Result<Response<Empty>, Status> {
        #[cfg(debug_assertions)]
        println!("UpdateUser");
        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();

        diesel::update(users)
            .filter(id.eq(&request.id))
            .filter(deleted.is_null())
            .set((name.eq(&request.name), avatar_id.eq(request.avatar_id)))
            .execute(&mut conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        println!("Elapsed: {:?}", start.elapsed());
        Ok(Response::new(Empty {}))
    }

    async fn update_payment_id(
        &self,
        request: Request<PaymentId>,
    ) -> Result<Response<Empty>, Status> {
        #[cfg(debug_assertions)]
        println!("UpdatePaymentId");
        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();

        diesel::update(users)
            .filter(id.eq(&request.user_id))
            .set((payment_id.eq(&request.payment_id),))
            .execute(&mut conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        println!("Elapsed: {:?}", start.elapsed());
        Ok(Response::new(Empty {}))
    }
}