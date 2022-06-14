use std::collections::HashSet;
use std::collections::vec_deque::VecDeque;

use rocket::http::ext::IntoCollection;

use gazetteer::tree::{HashMapSearchTree, Match, MatchType, MultiTree, ResultSelection, SearchTree};
use gazetteer::util::{read_lines, split_with_indices};

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
    let mut tree = HashMapSearchTree::default();

    tree.insert(VecDeque::from(split_with_indices(String::from("Puffinus")).0), String::from("Puffinus"), String::from("URI:short"), MatchType::Full);
    tree.insert(VecDeque::from(split_with_indices(String::from("p. puffinus")).0), String::from("p. puffinus"), String::from("URI:abbrv"), MatchType::Full);

    let result = tree.search(
        "ABC Puffinus p. puffinus X Y Z",
        Option::from(3),
        ,
        Option::from(&ResultSelection::Longest),
    );
    println!("{:?}", result);
}

fn process_test_file(tree: &impl SearchTree, max_len: Option<i32>) {
    let max_len = max_len.or(Option::from(5)).unwrap() as usize;

    println!("Loading test file..");
    let text = read_lines("resources/216578.txt")
        .join(" ");

    process_test_output(tree.search(&text, Option::from(max_len), , Option::from(&ResultSelection::Last)));
}

fn process_test_output(results: Vec<(String, HashSet<Match>, usize, usize)>) {
    for result in results {
        println!("{:?} ({},{}): {:?}", result.0, result.2, result.3, result.1)
    }
}


#[test]
fn test_sample() {
    let mut tree = HashMapSearchTree::default();
    for (s, uri) in vec![("An example phrase", "uri:phrase"), ("An example", "uri:example")] {
        let s = String::from(s);
        let uri = String::from(uri);
        let v: VecDeque<String> = s.split(" ").collect();
        tree.insert(v, s, uri, MatchType::Full);
    }
    println!("{:?}", tree.traverse(String::from("An xyz").split(" ").collect::<VecDeque<String>>(), 0));
    println!("{:?}", tree.traverse(String::from("An example").split(" ").collect::<VecDeque<String>>(), 0));
    println!("{:?}", tree.traverse(String::from("An example phrase").split(" ").collect::<VecDeque<String>>(), 0));
}

#[test]
fn test_small_string_tree() {
    let mut tree = HashMapSearchTree::default();
    tree.load("resources/taxa.txt", false, false, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_string_tree() {
    let mut tree = HashMapSearchTree::default();
    tree.load("resources/BIOfid/*", false, false, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_tree() {
    let mut tree = HashMapSearchTree::default();
    tree.load("resources/BIOfid/*", false, false, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_balanced() {
    let mut filter_list = read_lines("resources/filter_de.txt");
    filter_list.sort_unstable();

    let mut tree = MultiTree::default();
    tree.load("resources/taxa/_current/taxon/*.list", false, true, Option::from(&filter_list));
    tree.load("resources/taxa/_current/vernacular/*.list", false, false, Option::from(&filter_list));
    let tree = tree;
    process_test_file(&tree, Option::from(5));
}