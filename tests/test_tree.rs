use std::collections::vec_deque::VecDeque;

use gazetteer::tree::{ResultSelection, SearchTree, StringTree};
use gazetteer::util::split_with_indices;

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

#[test]
fn test_sanitize() {
    let mut tree = StringTree::default();

    tree.insert(VecDeque::from(split_with_indices("Puffinus").0), String::from("URI:short"));
    tree.insert(VecDeque::from(split_with_indices("p. puffinus").0), String::from("URI:abbrv"));

    let result = tree.search(
        "ABC Puffinus p. puffinus X Y Z",
        Option::from(3),
        Option::from(&ResultSelection::Longest),
    );
    println!("{:?}", result);
}