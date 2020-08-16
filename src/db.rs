use actix_web::{get, web, HttpResponse, Responder, Result};
#[allow(unused_imports)]
use futures::stream::StreamExt;
use mongodb::{error::Result as MResult, Collection};

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_subjects);
}

pub struct DataBase {
    pub subjects: Collection,
    pub topics: Collection,
    pub learning_objectives: Collection,
}

#[get("subjects")]
pub async fn get_subjects(db: web::Data<DataBase>) -> Result<impl Responder> {
    match pull_subject_graph(db).await {
        Ok(subs) => Ok(HttpResponse::Ok().json::<Vec<shared::Subject>>(subs).await),
        Err(_) => Ok(HttpResponse::InternalServerError()
            .reason("pull-graph gave error")
            .await),
    }
}

async fn pull_subject_graph(db: web::Data<DataBase>) -> MResult<Vec<shared::Subject>> {
    let mut cursor = db.subjects.find(None, None).await?;

    // TODO: Do this idiomatically.
    let mut v = Vec::<shared::Subject>::new();
    while let Some(Ok(doc)) = cursor.next().await {
        let next_name = doc.get_str("name").unwrap().to_string();
        let next_field = shared::Field::ComputerScience;
        let next_topic = std::collections::HashSet::<shared::Topic>::new();
        v.push(shared::Subject {
            name: next_name,
            field: next_field,
            topics: next_topic,
        });
    }
    Ok(v)
}
