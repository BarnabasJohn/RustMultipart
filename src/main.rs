use std::io::Result;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;
use actix_web::{ HttpServer,
                 App,
                 HttpResponse,
                 HttpRequest,
                 web,
                 http::header::CONTENT_LENGTH };
use actix_multipart::Multipart;
use futures_util::TryStreamExt as _ ;
use mime::{ Mime, IMAGE_PNG, IMAGE_JPEG, IMAGE_GIF };
use uuid::Uuid;
use image::{ DynamicImage, imageops::FilterType };
use actix_cors::Cors;
use serde::{ Deserialize, Serialize};
use sqlx::{self, postgres::PgPoolOptions, Pool, Postgres, FromRow};


#[derive(Serialize, Deserialize, FromRow)]
struct PostImage {
    file: String,
}

#[derive(Serialize, Deserialize, FromRow)]
struct GetImage {
    id: i32,
    image: Vec<u8>,
}


#[actix_web::main]
async fn main() -> Result<()> {
    if !Path::new("./upload").exists() {
        fs::create_dir("./upload").await?;
    }

    HttpServer::new( move || { let cors = Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .max_age(3600);

        App::new()
            .wrap(cors)
            .route("/", web::get().to(root))
            .route("/upload", web::post().to(upload))//converts and stores as GIF
            .route("/upload2", web::post().to(upload2))//locally stores images as loaded(jpeg, png)
            .route("/upload3", web::post().to(upload3))//stores loaded(jpeg, png) images to database
    }).bind(("127.0.0.1", 8080))?
        .run()
        .await
}

async fn root() -> String {
    "Server is up and running.".to_string()
}

//upload function converts and saves images in GIF
async fn upload(mut payload: Multipart, req: HttpRequest) -> HttpResponse {
    // 1. limit file size             done
    // 2. limit file count            done
    // 3. limit file type             done
    // 4. check if correct field      done
    // 5. convert to *gif             done
    // 6. save under random name      done
    let content_length: usize = match req.headers().get(CONTENT_LENGTH) {
        Some(header_value) => header_value.to_str().unwrap_or("0").parse().unwrap(),
        None => "0".parse().unwrap(),
    };

    let max_file_count: usize = 3;
    let max_file_size: usize = 10_000_000;//10MB
    let legal_filetypes: [Mime; 3] = [IMAGE_PNG, IMAGE_JPEG, IMAGE_GIF];
    let mut current_count: usize = 0;
    let dir: &str = "./upload/";

    if content_length > max_file_size { return HttpResponse::BadRequest().into(); }

    loop {
        if current_count == max_file_count { break; }
        if let Ok(Some(mut field)) = payload.try_next().await {
            let filetype: Option<&Mime> = field.content_type();
            if filetype.is_none() { continue; }
            if !legal_filetypes.contains(&filetype.unwrap()) { continue; }
            if field.name() != "avatar" { continue; }

            // println!("content_length: {:#?}", content_length);
            // println!("{}. picture:", current_count);
            // println!("name {}", field.name()); // &str
            // println!("headers {}", field.headers());
            // println!("content type {}", field.content_type()); // &Mime
            // println!("content type is mime::IMAGE_PNG {}", field.content_type() == &IMAGE_PNG);

            // println!("content disposition {}", field.content_disposition()); // &ContentDisposition

            // println!("filename {}", field.content_disposition().get_filename().unwrap()); // Option<&str>
            
            let destination: String = format!(
                "{}{}-{}",
                dir,
                Uuid::new_v4(),
                field.content_disposition().get_filename().unwrap()
            );

            let mut saved_file: fs::File = fs::File::create(&destination).await.unwrap();
            while let Ok(Some(chunk)) = field.try_next().await {
                let _ = saved_file.write_all(&chunk).await.unwrap();
            }

            web::block(move || async move {
                let uploaded_img: DynamicImage = image::open(&destination).unwrap();
                let _ = fs::remove_file(&destination).await.unwrap();
                uploaded_img
                    .resize_exact(200, 200, FilterType::Gaussian)
                    .save(format!("{}{}.gif", dir, Uuid::new_v4().to_string())).unwrap();
                println!("uploaded")
            }).await.unwrap().await;

        } else { break; }
        current_count += 1;
    }

    HttpResponse::Ok().into()
}

//Multipart forms can handle all medias of data
//this includes files, images and videos
//upload2 saves files locally as loaded(jpg, png)
async fn upload2( mut payload: Multipart, req: HttpRequest) -> HttpResponse {

    //1. Limit file size
    let total_content_length: usize = match req.headers().get(CONTENT_LENGTH) {
        Some(header_value) => header_value.to_str().unwrap_or("0").parse().unwrap(),
        None => "0".parse().unwrap(),
    };

    let max_file_size: usize = 10_000_000;//10MB total
    println!("received payload1");
    println!("content length: {}", total_content_length);

    if total_content_length > max_file_size { return HttpResponse::BadRequest().into(); }
    //-----end of 1
    println!("received payload2");

    let legal_filetypes: [Mime; 3] = [IMAGE_PNG, IMAGE_JPEG, IMAGE_GIF];

    let max_file_count: usize = 3;

    let mut current_count: usize = 0;

    loop {
        // 2. limit file count
        println!("received payload in loop");
        if current_count == max_file_count { break; }
         //-----end of 2

        if let Ok(Some(item)) = payload.try_next().await{
            let mut field = item;

            //3. limit file type
            let filetype: Option<&Mime> = field.content_type();
            if filetype.is_none() { continue; }
            if !legal_filetypes.contains(&filetype.unwrap()) { continue; }
            //-----end of 3

            println!("received payload");

            // 4. save under random name
            let destination: String = format!(
                "./upload/{}",
                field.content_disposition().get_filename().unwrap()
            );

            let mut create_file = fs::File::create(&destination).await.unwrap();

            if let Ok(Some(chunk)) = field.try_next().await {
                let data = chunk;
                println!("payload read");
                create_file.write_all(&data).await.unwrap();
            }
        }

        current_count += 1;

        println!("uploaded");
    }
    
    HttpResponse::Ok().body("File saved successfully")  
}

async fn upload3( state: Data<AppState>, mut payload: Multipart, req: HttpRequest) -> HttpResponse {

    //1. Limit file size
    let total_content_length: usize = match req.headers().get(CONTENT_LENGTH) {
        Some(header_value) => header_value.to_str().unwrap_or("0").parse().unwrap(),
        None => "0".parse().unwrap(),
    };

    let total_max_file_size: usize = 10_000_000;//10MB total
    println!("received payload1");
    println!("content length: {}", total_content_length);

    if total_content_length > total_max_file_size { return HttpResponse::BadRequest().into(); }
    //-----end of 1
    println!("received payload2");

    let legal_filetypes: [Mime; 3] = [IMAGE_PNG, IMAGE_JPEG, IMAGE_GIF];

    let max_file_count: usize = 3;

    let mut current_count: usize = 0;

    loop {
        // 2. limit file count
        println!("received payload in loop");
        if current_count == max_file_count { break; }
         //-----end of 2

        if let Ok(Some(item)) = payload.try_next().await{
            let mut field = item;

            //3. limit file type
            let filetype: Option<&Mime> = field.content_type();
            if filetype.is_none() { continue; }
            if !legal_filetypes.contains(&filetype.unwrap()) { continue; }
            //-----end of 3

            println!("received payload");

            // 4. save to database table

            if let Ok(Some(chunk)) = field.try_next().await {
                let data = chunk;

                let column = format!("image{}", current_count );
                
                match sqlx::query_as::<_, PostImage>(
                    "INSERT INTO images ($1) VALUES ($1) RETURNING id"
                )
                    .bind(column)
                    .bind(&*data)
                    .fetch_one(&state.db)
                    .await
                {
                    Ok(_) => HttpResponse::Ok().body("File saved to db successfully"),
                    Err(_) => HttpResponse::InternalServerError().json("Failed to save image to db"),
                };
            }
        }

        current_count += 1;

        println!("uploaded");
    }
    
    HttpResponse::Ok().body("File saved successfully")  
}