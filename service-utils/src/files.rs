use crate::db::{create_file, delete_file, get_file_by_id, get_files_by_target_id};
use crate::models::{DieselFile, InsertFile};
use crate::proto::utils_service_server::UtilsService;
use crate::proto::{File, FileId, FileType, TargetId};
use crate::MyService;
use anyhow::Result;
use futures_util::StreamExt;
use google_cloud_default::WithAuthExt;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::delete::DeleteObjectRequest;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use uuid::Uuid;

impl TryFrom<DieselFile> for File {
    type Error = Status;

    fn try_from(file: DieselFile) -> Result<Self, Self::Error> {
        let file = File {
            id: file.id.to_string(),
            created: file.created.to_string(),
            updated: file.updated.to_string(),
            deleted: file.deleted.map(|d| d.to_string()),
            target_id: file.target_id.to_string(),
            name: file.name,
            r#type: FileType::from_str_name(&file.type_)
                .ok_or(Status::internal("Invalid file type"))?
                .into(),
            buffer: Vec::new(),
            url: "".to_string(),
        };
        Ok(file)
    }
}

impl TryFrom<Result<DieselFile, diesel::result::Error>> for File {
    type Error = Status;

    fn try_from(file: Result<DieselFile, diesel::result::Error>) -> Result<Self, Self::Error> {
        let file = match file {
            Ok(file) => file,
            Err(e) => return Err(Status::internal(e.to_string())),
        };
        let file: File = match File::try_from(file) {
            Ok(file) => file,
            Err(e) => return Err(e),
        };
        Ok(file)
    }
}

#[tonic::async_trait]
impl UtilsService for MyService {
    type GetFilesStream = ReceiverStream<Result<File, Status>>;

    async fn get_files(
        &self,
        request: Request<TargetId>,
    ) -> Result<Response<Self::GetFilesStream>, Status> {
        #[cfg(debug_assertions)]
        println!("GetFiles: {:?}", request);
        let start = std::time::Instant::now();

        let pool = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();
        let target_uuid =
            Uuid::parse_str(&request.target_id).map_err(|e| Status::internal(e.to_string()))?;
        let r#type = FileType::from_i32(request.r#type)
            .ok_or(Status::internal("Invalid file type"))?
            .as_str_name();
        let mut files = get_files_by_target_id(pool, target_uuid, r#type).await?;

        let (tx, rx) = mpsc::channel(4);
        tokio::spawn(async move {
            while let Some(file) = files.next().await {
                let mut file: File = match File::try_from(file) {
                    Ok(file) => file,
                    Err(e) => {
                        tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                        break;
                    }
                };
                let env = std::env::var("ENV").unwrap();
                let bucket = std::env::var("BUCKET").unwrap();
                let mut buffer = Vec::new();
                if env == "development" {
                    let file_path = format!("/app/files/{}/{}", &file.id, &file.name);
                    buffer = match std::fs::read(file_path) {
                        Ok(buffer) => buffer,
                        Err(e) => {
                            tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                            break;
                        }
                    };
                } else if env == "production" {
                    let config = match ClientConfig::default().with_auth().await {
                        Ok(config) => config,
                        Err(e) => {
                            tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                            break;
                        }
                    };
                    let client = Client::new(config);
                    let data = client
                        .download_object(
                            &GetObjectRequest {
                                bucket,
                                object: format!("{}/{}", &file.id, &file.name),
                                ..Default::default()
                            },
                            &Range::default(),
                        )
                        .await;
                    buffer = match data {
                        Ok(data) => data,
                        Err(e) => {
                            tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
                            break;
                        }
                    };
                }
                file.buffer = buffer;
                tx.send(Ok(file)).await.unwrap();
            }
            println!("Elapsed: {:.2?}", start.elapsed());
            // loop {
            //     match files.try_next().await {
            //         Ok(None) => {
            //             let elapsed = start.elapsed();
            //             println!("Elapsed: {:.2?}", elapsed);
            //             break;
            //         }
            //         Ok(file) => {
            //             let file: DieselFile = match file {
            //                 Some(file) => file,
            //                 None => {
            //                     tx.send(Err(Status::internal("No file found")))
            //                         .await
            //                         .unwrap();
            //                     break;
            //                 }
            //             };

            //             let env = std::env::var("ENV").unwrap();
            //             let bucket = std::env::var("BUCKET").unwrap();
            //             let mut buffer = Vec::new();
            //             if env == "development" {
            //                 let file_path = format!("/app/files/{}/{}", &file.id, &file.name);
            //                 buffer = match std::fs::read(file_path) {
            //                     Ok(buffer) => buffer,
            //                     Err(e) => {
            //                         tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            //                         break;
            //                     }
            //                 };
            //             } else if env == "production" {
            //                 let config = match ClientConfig::default().with_auth().await {
            //                     Ok(config) => config,
            //                     Err(e) => {
            //                         tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            //                         break;
            //                     }
            //                 };
            //                 let client = Client::new(config);
            //                 let data = client
            //                     .download_object(
            //                         &GetObjectRequest {
            //                             bucket,
            //                             object: format!("{}/{}", &file.id, &file.name),
            //                             ..Default::default()
            //                         },
            //                         &Range::default(),
            //                     )
            //                     .await;
            //                 buffer = match data {
            //                     Ok(data) => data,
            //                     Err(e) => {
            //                         tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            //                         break;
            //                     }
            //                 };
            //             }
            //             file.buffer = buffer;
            //             tx.send(Ok(file)).await.unwrap();
            //         }
            //         Err(e) => {
            //             tx.send(Err(Status::internal(e.to_string()))).await.unwrap();
            //             break;
            //         }
            //     }
            // }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_file(&self, request: Request<FileId>) -> Result<Response<File>, Status> {
        #[cfg(debug_assertions)]
        println!("GetFile: {:?}", request);
        let start = std::time::Instant::now();

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();
        let file_uuid =
            Uuid::parse_str(&request.file_id).map_err(|e| Status::internal(e.to_string()))?;
        let target_uuid =
            Uuid::parse_str(&request.target_id).map_err(|e| Status::internal(e.to_string()))?;

        let file = get_file_by_id(conn, file_uuid, target_uuid).await?;
        let mut file = File::try_from(file)?;

        let env = std::env::var("ENV").unwrap();
        let bucket = std::env::var("BUCKET").unwrap();
        let mut buffer = Vec::new();

        if env == "development" {
            let file_path = format!("/app/files/{}/{}", &file.id, &file.name);
            buffer = match std::fs::read(file_path) {
                Ok(buffer) => buffer,
                Err(e) => return Err(Status::internal(e.to_string())),
            };
        } else if env == "production" {
            let config = match ClientConfig::default().with_auth().await {
                Ok(config) => config,
                Err(e) => return Err(Status::internal(e.to_string())),
            };
            let client = Client::new(config);
            let data = client
                .download_object(
                    &GetObjectRequest {
                        bucket,
                        object: format!("{}/{}", &file.id, &file.name),
                        ..Default::default()
                    },
                    &Range::default(),
                )
                .await;
            buffer = match data {
                Ok(data) => data,
                Err(e) => return Err(Status::internal(e.to_string())),
            };
        }
        file.buffer = buffer;

        let elapsed = start.elapsed();
        println!("Elapsed: {:.2?}", elapsed);
        Ok(Response::new(file))
    }

    async fn create_file(&self, request: Request<File>) -> Result<Response<File>, Status> {
        #[cfg(debug_assertions)]
        println!("CreateFile");
        let start = std::time::Instant::now();

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // get file from request
        let file = request.into_inner();
        let file_buffer = file.buffer;
        let target_uuid =
            Uuid::parse_str(&file.target_id).map_err(|e| Status::internal(e.to_string()))?;
        let r#type = FileType::from_i32(file.r#type)
            .ok_or(anyhow::anyhow!("Invalid file type"))
            .map_err(|e| Status::internal(e.to_string()))?
            .as_str_name();

        // save file to db
        let new_file = InsertFile {
            name: &file.name,
            type_: r#type,
            target_id: &target_uuid,
        };
        let file: File = create_file(conn, new_file).await?.try_into()?;

        let env = std::env::var("ENV").unwrap();
        let bucket = std::env::var("BUCKET").unwrap();
        if env == "development" {
            // save file to disk
            let file_path = format!("/app/files/{}/{}", file.id, file.name);
            tokio::fs::create_dir_all(format!("/app/files/{}", file.id)).await?;
            let mut new_file = tokio::fs::File::create(file_path).await?;
            new_file.write_all(&file_buffer).await?;
        } else if env == "production" {
            // save to GCP storage
            let config = ClientConfig::default()
                .with_auth()
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            let client = Client::new(config);
            let file_path = format!("{}/{}", file.id, file.name);
            let upload_type = UploadType::Simple(Media::new(file_path));
            let uploaded = client
                .upload_object(
                    &UploadObjectRequest {
                        bucket: bucket.to_string(),
                        ..Default::default()
                    },
                    file_buffer,
                    &upload_type,
                )
                .await;
            if let Err(e) = uploaded {
                return Err(Status::internal(e.to_string()));
            }
        }
        println!("Elapsed: {:.2?}", start.elapsed());
        return Ok(Response::new(file));
    }

    async fn delete_file(&self, request: Request<FileId>) -> Result<Response<File>, Status> {
        #[cfg(debug_assertions)]
        println!("DeleteFile: {:?}", request);
        let start = std::time::Instant::now();

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let request = request.into_inner();
        let file_uuid =
            Uuid::parse_str(&request.file_id).map_err(|e| Status::internal(e.to_string()))?;
        let target_uuid =
            Uuid::parse_str(&request.target_id).map_err(|e| Status::internal(e.to_string()))?;

        let file: File = delete_file(conn, file_uuid, target_uuid)
            .await?
            .try_into()?;

        let env = std::env::var("ENV").unwrap();
        let bucket = std::env::var("BUCKET").unwrap();
        if env == "development" {
            // delete file from disk
            tokio::fs::remove_dir_all(format!("/app/files/{}", file.id)).await?;
        } else if env == "production" {
            // delete from GCP storage
            let config = ClientConfig::default()
                .with_auth()
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            let client = Client::new(config);
            let deleted = client
                .delete_object(&DeleteObjectRequest {
                    bucket: bucket.to_string(),
                    object: format!("{}/{}", file.id, file.name),
                    ..Default::default()
                })
                .await;
            if let Err(e) = deleted {
                return Err(Status::internal(e.to_string()));
            }
        }

        println!("Elapsed: {:.2?}", start.elapsed());
        return Ok(Response::new(file));
    }
}
