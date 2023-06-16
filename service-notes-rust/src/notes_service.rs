use crate::{
    proto::{notes_service_server::NotesService, Empty, Note, NoteId, UserId},
    MyService,
};
use anyhow::Result;
use futures_util::StreamExt;
use mysql_async::{from_row_opt, prelude::WithParams, Row};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use mysql_async::prelude::*;

impl TryFrom<Row> for Note {
    type Error = anyhow::Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let note = from_row_opt(row)?;
        Ok(note)
    }
}

impl FromRow for Note {
    fn from_row_opt(row: Row) -> Result<Self, mysql_async::FromRowError> {
        let (id, created, updated, deleted, user_id, title, content) =
            mysql_async::from_row_opt::<(
                Vec<u8>,
                time::Date,
                time::Date,
                Option<time::Date>,
                Vec<u8>,
                String,
                String,
            )>(row)?;
        let note = Note {
            id,
            created: created.to_string(),
            updated: updated.to_string(),
            deleted: deleted.map(|d| d.to_string()),
            user_id,
            title,
            content,
        };
        Ok(note)
    }
}

// impl TryFrom<DieselNote> for Note {
//     type Error = anyhow::Error;
//
//     fn try_from(note: DieselNote) -> Result<Self, Self::Error> {
//         let note = Note {
//             id: note.id,
//             created: note.created.to_string(),
//             updated: note.updated.to_string(),
//             deleted: note.deleted.map(|d| d.to_string()),
//             user_id: note.user_id,
//             title: note.title,
//             content: note.content,
//         };
//         Ok(note)
//     }
// }

#[tonic::async_trait]
impl NotesService for MyService {
    type GetNotesStream = ReceiverStream<Result<Note, Status>>;

    async fn get_notes(
        &self,
        request: Request<UserId>,
    ) -> Result<Response<Self::GetNotesStream>, Status> {
        #[cfg(debug_assertions)]
        println!("GetNotes");
        let start = std::time::Instant::now();

        // let conn = self
        //     .pool
        //     .get()
        //     .await
        //     .map_err(|e| Status::internal(e.to_string()))?;

        println!("Connect: {:?}", start.elapsed());

        let request = request.into_inner();

        // let notes = get_notes_by_user_uuid(conn, request.user_id)
        //     .await
        //     .map_err(|e| Status::internal(e.to_string()))?;

        // let notes = "SELECT * FROM notes where user_id = :user_id"
        //     .with(params! {
        //         "user_id" => request.user_id,
        //     })
        //     .map(
        //         &mut mysql_conn,
        //         |(id, created, updated, deleted, user_id, title, content)| MysqlNote {
        //             id,
        //             created,
        //             updated,
        //             deleted,
        //             user_id,
        //             title,
        //             content,
        //         },
        //     )
        //     .await
        //     .map_err(|e| Status::internal(e.to_string()))?;
        //

        let conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // let mut notes = mysql_conn
        //     .exec_iter(
        //         "SELECT * FROM notes where user_id = :user_id",
        //         params! {
        //             "user_id" => request.user_id,
        //         },
        //     )
        //     .await
        //     .map_err(|e| Status::internal(e.to_string()))?;

        let mut notes = "SELECT * FROM notes where user_id = :user_id"
            .with(params! {
                "user_id" => request.user_id,
            })
            .stream(conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        println!("Query: {:?}", start.elapsed());

        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            loop {
                let note = notes.next().await;
                let note = match note {
                    Some(note) => note,
                    None => {
                        break;
                    }
                };
                let note: Note = match note {
                    Ok(note) => note,
                    Err(e) => {
                        tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                        break;
                    }
                };
                let note: Note = match Note::try_from(note) {
                    Ok(note) => note,
                    Err(e) => {
                        tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                        break;
                    }
                };
                match tx.send(Ok(note)).await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Error: {:?}", e);
                        break;
                    }
                }
            }
            println!("Elapsed: {:.2?}", start.elapsed());
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn create_note(&self, request: Request<Note>) -> Result<Response<Empty>, Status> {
        #[cfg(debug_assertions)]
        println!("CreateNote");
        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let note = request.into_inner();

        r"delete from notes where user_id = :user_id"
            .with(params! {
                "user_id" => &note.user_id,
            })
            .run(&mut conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        for _ in 0..5000 {
            r"insert into notes (id, user_id, title, content) values (:id, :user_id, :title, :content) on duplicate key update title = :title, content = :content"
                .with(
                    params! {
                        "id" => &Uuid::now_v7().as_bytes().to_vec(),
                        "user_id" => &note.user_id,
                        "title" => &note.title,
                        "content" => &note.content,
                    }).run(&mut conn).await.map_err(|e| Status::internal(e.to_string()))?;
        }

        println!("Elapsed: {:.2?}", start.elapsed());
        drop(conn);
        return Ok(Response::new(Empty {}));
    }

    async fn delete_note(&self, request: Request<NoteId>) -> Result<Response<Empty>, Status> {
        #[cfg(debug_assertions)]
        println!("DeleteNote");
        let start = std::time::Instant::now();

        let mut conn = self
            .pool
            .get_conn()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();

        r"delete from notes where id = :id and user_id = :user_id"
            .with(params! {
                "id" => request.note_id,
                "user_id" => request.user_id,
            })
            .run(&mut conn)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        println!("Elapsed: {:.2?}", start.elapsed());
        drop(conn);
        return Ok(Response::new(Empty {}));
    }
}