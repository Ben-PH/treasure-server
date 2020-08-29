use actix_files::{Files, NamedFile};
use actix_identity::{CookieIdentityPolicy, Identity, IdentityService};
use actix_web::{middleware, web, App, HttpServer, Result};
use ring::rand::SystemRandom;

mod auth;
mod db;
use rand::Rng;
use ring::rand::SecureRandom;

use mongodb::bson::{self, doc};
use serde::{Deserialize, Serialize};

// static PWD_DB_SALT: &[u8; 16] = b"database spicey!";

pub struct AppData {
    rng: SystemRandom,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserDocument {
    _id: bson::oid::ObjectId,
    first_name: String,
    last_name: String,
    email: String,
    password: String,
}

async fn index(id: Identity) -> Result<NamedFile> {
    println!("your id is {:?}", id.identity());
    Ok(NamedFile::open("./client/index.html")?)
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=info");
    env_logger::init();
    let db = mongodb::Client::with_uri_str("mongodb://localhost:27017")
        .await
        .unwrap()
        .database("treasure_mind");
    let adb_conn: std::sync::Arc<arangors::Connection> = std::sync::Arc::new(
        arangors::Connection::establish_without_auth("http://localhost:8529")
            .await
            .unwrap(),
    );
    HttpServer::new(move || {
        // First fill is high-latency. so do it one time round
        let rng = ring::rand::SystemRandom::new();
        {
            let mut tmp_var = [0u8; 16];
            if let Err(e) = rng.fill(&mut tmp_var) {
                eprintln!("rng initialization fill() error: {:?}", e);
            }
        }
        let private_key = rand::thread_rng().gen::<[u8; 32]>();
        let users = db.collection("users");
        let pwds = db.collection("passwords");

        let subjects = db.collection("subjects");
        let topics = db.collection("topics");
        let learning_objectives = db.collection("learning_objectives");

        App::new()
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&private_key)
                    .name("Authorization")
                    .max_age(60 * 10)
                    .secure(true),
            ))
            .wrap(middleware::Logger::default())
            .data(auth::DbCollections::init(users))
            .data(auth::PwdDb::init(pwds))
            .data(adb_conn.clone())
            .data(AppData { rng })
            .data(db::DataBase {
                subjects,
                topics,
                learning_objectives,
            })
            .service(
                web::scope("/api/auth")
                    .configure(auth::config)
                    .default_service(web::route().to(web::HttpResponse::NotFound)),
            )
            .service(web::scope("/api/graph").configure(db::config))
            .service(Files::new("/pkg", "./client/pkg"))
            .service(Files::new("/", "./client/static").index_file("index.html"))
            .default_service(web::get().to(index))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
