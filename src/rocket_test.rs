use std::collections::vec_deque::VecDeque;

use rocket::http::{Accept, ContentType, Status};
use rocket::local::blocking::Client;
use rocket::serde::{Deserialize, Serialize, uuid::Uuid};

use gazetteer::tree::{ResultSelection, SearchTree, StringTree};
use gazetteer::util::split_with_indices;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Request {
    text: String,
    max_len: Option<usize>,
    result_selection: Option<String>,
}

impl Request {
    fn new(text: impl Into<String>) -> Self {
        Request { text: text.into(), max_len: None, result_selection: None }
    }
}

#[test]
fn json_tag() {
    let client = Client::tracked(super::rocket()).unwrap();

    let message = Request::new("Nach Schluß des Congresses ist eine längere Excursion vorgesehen, auf welcher die Inseln an der Küste von Pembrokshire besucht werden.
Dieser Ausflug dürfte besonders interessant werden, weil sich hier große Brutkolonien von Puffinus p. puffinus und verschiedener Alcidae befinden.
Auch Thalassidroma pelagica dürfte hier angetroffen werden.
Bei günstigem Wetter ist ferner der Besuch einer Brutkolonie von Sula bassana vorgesehen.");
    let res = client.post("/tag").json(&message).dispatch();
    assert_eq!(res.status(), Status::Ok);
    println!("{:?}", res.body())
}

// #[test]
// fn json_bad_get_put() {
//     let client = Client::tracked(super::rocket()).unwrap();
//
//     // Try to get a message with an ID that doesn't exist.
//     let res = client.get("/json/99").header(ContentType::JSON).dispatch();
//     assert_eq!(res.status(), Status::NotFound);
//
//     let body = res.into_string().unwrap();
//     assert!(body.contains("error"));
//     assert!(body.contains("Resource was not found."));
//
//     // Try to get a message with an invalid ID.
//     let res = client.get("/json/hi").header(ContentType::JSON).dispatch();
//     assert_eq!(res.status(), Status::NotFound);
//     assert!(res.into_string().unwrap().contains("error"));
//
//     // Try to put a message without a proper body.
//     let res = client.put("/json/80").header(ContentType::JSON).dispatch();
//     assert_eq!(res.status(), Status::BadRequest);
//
//     // Try to put a message with a semantically invalid body.
//     let res = client.put("/json/0")
//         .header(ContentType::JSON)
//         .body(r#"{ "dogs?": "love'em!" }"#)
//         .dispatch();
//
//     assert_eq!(res.status(), Status::UnprocessableEntity);
//
//     // Try to put a message for an ID that doesn't exist.
//     let res = client.put("/json/80")
//         .json(&Message::new("hi"))
//         .dispatch();
//
//     assert_eq!(res.status(), Status::NotFound);
// }