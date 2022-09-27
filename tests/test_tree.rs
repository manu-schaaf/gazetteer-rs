use std::collections::HashSet;
use std::collections::vec_deque::VecDeque;

use rocket::http::ext::IntoCollection;

use gazetteer::tree::{HashMapSearchTree, Match, MatchType, ResultSelection, SearchTree};
use gazetteer::util::{read_lines};

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

    tree.insert(VecDeque::from(vec!["Puffinus".to_string()]), String::from("Puffinus"), String::from("URI:short"), MatchType::Full);
    tree.insert(VecDeque::from(vec!["p.".to_string(), "puffinus".to_string()]), String::from("p. puffinus"), String::from("URI:abbrv"), MatchType::Full);

    let result = tree.search(
        "ABC Puffinus p. puffinus X Y Z",
        Option::from(3),
        Option::from(&ResultSelection::Longest),
    );
    println!("{:?}", result);
}

fn process_test_file(tree: &impl SearchTree, max_len: Option<i32>) {
    let max_len = max_len.or(Option::from(5)).unwrap() as usize;

    println!("Loading test file..");
    let text = read_lines("resources/216578.txt")
        .join(" ");

    process_test_output(tree.search(&text, Option::from(max_len), Option::from(&ResultSelection::Last)));
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
        let v: VecDeque<String> = s.split(" ").map(|s| String::from(s)).collect();
        tree.insert(v, s, uri, MatchType::Full);
    }
    println!("{:?}", tree.traverse(String::from("An xyz").split(" ").map(|s| String::from(s)).collect::<VecDeque<String>>()));
    println!("{:?}", tree.traverse(String::from("An example").split(" ").map(|s| String::from(s)).collect::<VecDeque<String>>()));
    println!("{:?}", tree.traverse(String::from("An example phrase").split(" ").map(|s| String::from(s)).collect::<VecDeque<String>>()));
}

#[test]
fn test_small_string_tree() {
    let mut tree = HashMapSearchTree::default();
    tree.load("resources/taxa.txt", false, false, None, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_string_tree() {
    let mut tree = HashMapSearchTree::default();
    tree.load("resources/BIOfid/*", false, false, None, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_tree() {
    let mut tree = HashMapSearchTree::default();
    tree.load("resources/BIOfid/*", false, false, None, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_match_sort() {
    let mut mtches = vec![
        Match {
            match_type: MatchType::Abbreviated,
            match_string: "1_FULL".to_string(),
            match_label: "1_URI".to_string(),
        },
        Match {
            match_type: MatchType::Abbreviated,
            match_string: "1_ABBRV".to_string(),
            match_label: "2_URI".to_string(),
        },
        Match {
            match_type: MatchType::NGram,
            match_string: "1_NGRAM".to_string(),
            match_label: "3_URI".to_string(),
        },
        Match {
            match_type: MatchType::Full,
            match_string: "1_FULL".to_string(),
            match_label: "1_URI".to_string(),
        },
        Match {
            match_type: MatchType::None,
            match_string: "_".to_string(),
            match_label: "_".to_string(),
        },
    ];
    mtches.sort();
    println!("{:?}", mtches);
}