use actix_web::{get, web, HttpResponse, Responder, Result};
#[allow(unused_imports)]
use futures::stream::StreamExt;
use mongodb::{error::Result as MResult, Collection};
use shared::{Field, Subject, SubjectId, Topic, TopicId};
use std::collections::HashMap;
use std::rc::Rc;

pub fn config(cfg: &mut web::ServiceConfig) {
    cfg.service(get_subjects).service(get_topics);
}

pub struct DataBase {
    pub subjects: Collection,
    pub topics: Collection,
    pub learning_objectives: Collection,
}

#[get("topics/{subj_id}")]
pub async fn get_topics(
    db: web::Data<DataBase>,
    subj_id: web::Path<SubjectId>,
) -> Result<impl Responder> {
    match get_topic_list(db, *subj_id).await {
        Ok(subs) => Ok(HttpResponse::Ok()
            .json::<HashMap<TopicId, Topic>>(subs)
            .await),
        Err(_) => Ok(HttpResponse::InternalServerError()
            .reason("pull-graph gave error")
            .await),
    }
}
async fn get_topic_list(
    db: web::Data<DataBase>,
    subj_id: SubjectId,
) -> MResult<HashMap<TopicId, Topic>> {
    let subj = db
        .subjects
        .find_one(bson::doc! {"id": subj_id}, None)
        .await?
        .unwrap();
    let top_objs = subj.get("topics").unwrap();
    let mut arr = db
        .topics
        .find(bson::doc! {"_id" : {"$in" : top_objs}}, None)
        .await?;
    let mut v: HashMap<TopicId, Topic> = HashMap::new();
    while let Some(Ok(topic)) = arr.next().await {
        let id = topic.get_i32("id").unwrap();
        let name = topic.get_str("name").unwrap();
        println!("topic: {:?}", topic);
        let mut new = Topic::init(id, name.to_string());
        let tasks = topic.get("tasks").unwrap();

        let mut tops = db
            .as_ref()
            .learning_objectives
            .find(bson::doc! {"_id" : {"$in" : tasks}}, None)
            .await
            .unwrap();

        println!("tasks: {:#?}", tasks);
        while let Some(Ok(lo)) = tops.next().await {
            let id = lo.get_i32("id").unwrap();
            let name = lo.get_str("name").unwrap();
            let task = lo.get_str("task").unwrap();
            let mut new_lo = shared::LearningObj::init(id, name.to_string(), task.to_string());
            if let bson::Bson::Array(hints) = lo.get("hints").unwrap() {
                for hint in hints {
                    new_lo.hints.push(hint.to_string());
                    println!("hint: {:#?}", hint);
                }
            }
            new.learning_objectives.insert(id, Rc::new(new_lo));
        }
        v.insert(id, new);
    }
    Ok(v)
}
#[get("subjects")]
pub async fn get_subjects(db: web::Data<DataBase>) -> Result<impl Responder> {
    match pull_subject_graph(db).await {
        Ok(subs) => Ok(HttpResponse::Ok()
            .json::<HashMap<SubjectId, Subject>>(subs)
            .await),
        Err(_) => Ok(HttpResponse::InternalServerError()
            .reason("pull-graph gave error")
            .await),
    }
}

async fn pull_subject_graph(db: web::Data<DataBase>) -> MResult<HashMap<SubjectId, Subject>> {
    let mut cursor = db.subjects.find(None, None).await?;

    // TODO: Do this idiomatically.
    let mut v: HashMap<SubjectId, Subject> = HashMap::new();
    while let Some(Ok(doc)) = cursor.next().await {
        let id = doc.get_i32("id").unwrap();
        let next_name = doc.get_str("name").unwrap().to_string();
        let next_field = Field::ComputerScience;
        let next_topic = std::collections::HashMap::<TopicId, Rc<Topic>>::new();
        v.insert(
            id,
            Subject {
                id,
                name: next_name,
                field: next_field,
                topics: next_topic,
            },
        );
    }
    Ok(v)
}
