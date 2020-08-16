use std::num::NonZeroU32;

use actix_identity::Identity;
use actix_web::{delete, get, post, web, HttpResponse, Responder, Result};
use mongodb::bson::{doc, spec, Binary, Bson};
use ring::rand::{SecureRandom, SystemRandom};

const CREDENTIAL_LEN: usize = ring::digest::SHA256_OUTPUT_LEN;
const VER_ITER: u32 = 1000;

pub struct Credential([u8; CREDENTIAL_LEN]);
static PBKDF2_ALG: ring::pbkdf2::Algorithm = ring::pbkdf2::PBKDF2_HMAC_SHA256;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(create)
        .service(profile)
        .service(login)
        .service(logout);
}

pub struct DbCollections {
    users: mongodb::Collection,
}

impl DbCollections {
    pub fn init(users: mongodb::Collection) -> Self {
        Self { users }
    }
}

pub struct PwdDb {
    pbkdf2_iters: NonZeroU32,
    storage: mongodb::Collection,
}

impl PwdDb {
    /// See [this stack-overflow](https://stackoverflow.com/a/674931) question on salting
    fn fill_salt(
        &self,
        rng: &SystemRandom,
        barray: &mut [u8],
    ) -> Result<(), ring::error::Unspecified> {
        rng.fill(barray)
    }
    pub fn init(storage: mongodb::Collection) -> Self {
        Self {
            pbkdf2_iters: NonZeroU32::new(VER_ITER).unwrap(),
            storage,
        }
    }
}

#[post("/create")]
async fn create(
    reg_data: web::Json<shared::Register>,
    app_data: web::Data<crate::AppData>,
    user_db: web::Data<DbCollections>,
    pswd_db: web::Data<PwdDb>,
) -> Result<impl Responder> {
    let user_db = user_db.into_inner();
    let reg_data = reg_data.into_inner();
    if let Ok(Some(_)) = user_db
        .users
        .find_one(doc! {"email": &reg_data.email}, None)
        .await
    {
        return HttpResponse::Unauthorized()
            .reason("email already exists")
            .await;
    }

    // TODO CRUCIAL do these the rusty way. if it returned Err, we still go ahead :(
    let pswd_db = pswd_db.into_inner();
    if let Ok(Some(_)) = pswd_db
        .storage
        .find_one(doc! {"email": &reg_data.email}, None)
        .await
    {
        return HttpResponse::InternalServerError()
            .reason("inconsistent database")
            .await;
    }

    let mut salt = [0u8; CREDENTIAL_LEN];
    if pswd_db
        .fill_salt(&app_data.into_inner().rng, &mut salt)
        .is_err()
    {
        return HttpResponse::InternalServerError().await;
    }
    let mut to_store = Credential([0u8; CREDENTIAL_LEN]);
    ring::pbkdf2::derive(
        PBKDF2_ALG,
        pswd_db.pbkdf2_iters,
        &salt,
        &reg_data.password.as_bytes(),
        &mut to_store.0,
    );
    let hash = Binary {
        subtype: spec::BinarySubtype::Generic,
        bytes: to_store.0.to_vec(),
    };

    // TODO handle bad insertion
    let pwd_storage = &pswd_db.storage;
    let db_storage = &user_db.users;

    match pwd_storage
        .insert_one(
            doc! {
                "email": &reg_data.email,
                "hashed": &hash,
            },
            None,
        )
        .await
    {
        Ok(_) => {}
        Err(_) => return HttpResponse::InternalServerError().await,
    }

    // TODO handle bad insertion
    db_storage
        .insert_one(
            doc! {
                "first_name": reg_data.first_name,
                "last_name": reg_data.last_name,
                "email": reg_data.email,
                // TODO: link to password hashing document
                "salt": Binary{subtype: spec::BinarySubtype::Generic, bytes: salt.to_vec()}
            },
            None,
        )
        .await;
    HttpResponse::Ok().await
}

#[post("/login")]
async fn login(
    id: Identity,
    auth_data: web::Json<shared::Login>,
    passwords: web::Data<PwdDb>,
    col: web::Data<DbCollections>,
) -> Result<impl Responder> {
    let auth = auth_data.into_inner();
    let pwds = passwords.into_inner();
    let col = col.into_inner();
    let hash = match pwds
        .storage
        .find_one(doc! {"email": &auth.email}, None)
        .await
    {
        Ok(Some(mut doc)) => match doc.remove("hashed") {
            Some(Bson::Binary(bin)) => {
                println!("{:#?}", doc);
                Some(bin.bytes)
            }

            _ => None,
        },
        _ => None,
    };

    let user = col.users.find_one(doc! {"email": &auth.email}, None).await;
    let salt = match user {
        Ok(Some(mut doc)) => match doc.remove("salt") {
            Some(Bson::Binary(bin)) => {
                println!("{:?}", doc);
                Some(bin.bytes)
            }

            _ => None,
        },
        _ => None,
    };

    // println!("{:?}", salt);
    // println!("{:?}", hash);

    // pull out the password hash and users salt from the database
    // TODO don't do unwrap in a scop guarded by is_some(). do it the rusty way.
    let mut user = match col.users.find_one(doc! {"email": &auth.email}, None).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return HttpResponse::Unauthorized()
                .reason("couldn't find the users email")
                .await
        }
        _ => return HttpResponse::InternalServerError().await,
    };

    user.remove("_id");
    user.remove("salt");
    if salt.is_some()
        && hash.is_some()
        && ring::pbkdf2::verify(
            PBKDF2_ALG,
            pwds.pbkdf2_iters,
            salt.unwrap().as_slice(),
            auth.password.as_bytes(),
            hash.unwrap().as_slice(),
        )
        .is_ok()
    {
        id.remember(auth.email);
        HttpResponse::Ok()
            .reason("you are logged in")
            .json(user)
            .await
    } else {
        HttpResponse::Unauthorized()
            .reason("invalid username or password")
            .await
    }
}

#[delete("")]
async fn logout(id: Identity) -> Result<impl Responder> {
    match id.identity() {
        Some(email) => {
            println!("goodbye, {:?}", email);
            id.forget();
            HttpResponse::Ok().finish().await
        }
        None => {
            println!("sorry, who are you?");
            HttpResponse::Unauthorized()
                .reason("invalid Authorization cookie")
                .await
        }
    }
}
#[get("")]
async fn profile(id: Identity, users: web::Data<DbCollections>) -> Result<impl Responder> {
    println!("auth get id: {:?}", id.identity());
    match id.identity() {
        Some(email) => {
            let mut user = users
                .into_inner()
                .users
                .find_one(doc! {"email": &email}, None)
                .await
                .unwrap()
                .unwrap();
            id.remember(email);
            user.remove("_id");
            user.remove("salt");
            HttpResponse::Ok().json(user).await
        }
        None => {
            HttpResponse::Unauthorized()
                .reason("no-good auth cookie")
                .await
        }
    }
}
