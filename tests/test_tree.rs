use std::collections::HashSet;
use std::collections::vec_deque::VecDeque;

use rocket::http::ext::IntoCollection;

use gazetteer::tree::{BinarySearchTree, Match, MatchType, MultiTree, ResultSelection, SearchTree};
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
    let mut tree = BinarySearchTree::default();

    tree.insert(VecDeque::from(split_with_indices("Puffinus").0), String::from("Puffinus"), String::from("URI:short"), MatchType::Full);
    tree.insert(VecDeque::from(split_with_indices("p. puffinus").0), String::from("p. puffinus"), String::from("URI:abbrv"), MatchType::Full);

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
    let mut tree = BinarySearchTree::default();
    for (s, uri) in vec![("An example phrase", "uri:phrase"), ("An example", "uri:example")] {
        let s = String::from(s);
        let uri = String::from(uri);
        let v: VecDeque<&str> = s.split(' ').collect::<VecDeque<&str>>();
        tree.insert(v, s, uri, MatchType::Full);
    }
    println!("{:?}", tree.traverse(String::from("An xyz").split(' ').collect::<VecDeque<&str>>()));
    println!("{:?}", tree.traverse(String::from("An example").split(' ').collect::<VecDeque<&str>>()));
    println!("{:?}", tree.traverse(String::from("An example phrase").split(' ').collect::<VecDeque<&str>>()));
}

#[test]
fn test_small_string_tree() {
    let mut tree = BinarySearchTree::default();
    tree.load("resources/taxa.txt", false, false, 0, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_string_tree() {
    let mut tree = BinarySearchTree::default();
    tree.load("resources/BIOfid/*", false, false, 0, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_tree() {
    let mut tree = BinarySearchTree::default();
    tree.load("resources/BIOfid/*", false, false, 0, None);
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_balanced() {
    // let tree = MultiTree::load_balanced("resources/BIOfid/taxa_*", 500_000);
    let mut filter_list = read_lines("resources/filter_de.txt");
    filter_list.sort_unstable();

    let mut tree = MultiTree::default();
    tree.load("resources/taxa/_current/taxon/*.list", false, true, 0, Option::from(&filter_list));
    tree.load("resources/taxa/_current/vernacular/*.list", false, false, 0, Option::from(&filter_list));
    let tree = tree;
    process_test_file(&tree, Option::from(5));
}

//
// #[test]
// fn test_small() {
//     let max_len = 5;
//     let tree = load("resources/taxa.txt".to_string());
//
//     println!("Loading test file..");
//     let text = read_lines("resources/216578.txt").unwrap()
//         .map(|line| line.unwrap().trim().to_string())
//         .collect::<Vec<String>>()
//         .join(" ");
//     let (offsets, slices) = split_with_indices(&text);
//
//     println!("Iterating over all words..");
//     let results: Vec<Result<Vec<_>, _>> = slices.par_windows(max_len)
//         .map(|slice| tree.traverse(VecDeque::from(slice.to_vec())))
//         .collect();
//
//     offsets.windows(max_len).into_iter().zip(results.into_iter()).for_each(
//         |(offsets, results)| if let Ok(results) = results {
//             let start = offsets[0].0;
//             for result in results {
//                 let end = offsets[result.1.len() - 1].1;
//                 println!("{:?} ({},{}) -> {:}", result.1.join(" "), start, end, result.0)
//             }
//         }
//     )
//     // {
//     //     if let Ok(result) = tree.traverse(VecDeque::from(slice.clone())) {
//     //         println!("Default: '{}' -> {:?}", slice.clone().join(" "), result);
//     //     }
//     // }
// }
//
// #[test]
// fn test_large_single() {
//     let max_len = 5;
//     let tree = load("resources/taxa/".to_string());
//
//     println!("Loading test file..");
//     let text = read_lines("resources/216578.txt").unwrap()
//         .map(|line| line.unwrap().trim().to_string())
//         .collect::<Vec<String>>()
//         .join(" ");
//     let (offsets, slices) = split_with_indices(&text);
//
//     println!("Iterating over all words..");
//     let results: Vec<Result<Vec<_>, _>> = slices.par_windows(max_len)
//         .map(|slice| tree.traverse(VecDeque::from(slice.to_vec())))
//         .collect();
//
//     offsets.windows(max_len).into_iter().zip(results.into_iter()).for_each(
//         |(offsets, results)| if let Ok(results) = results {
//             let start = offsets[0].0;
//             for result in results {
//                 let end = offsets[result.1.len() - 1].1;
//                 println!("{:?} ({},{}) -> {:}", result.1.join(" "), start, end, result.0)
//             }
//         }
//     )
// }
//
//
//
// #[test]
// fn test_symspell_small_taxa() {
//     let mut max_len = 5;
//     let (tree, symspell) = load_symspell("resources/taxa.txt".to_string(), "resources/de-100k.txt");
//
//     println!("Loading test file..");
//     let text = read_lines("resources/216578.txt").unwrap()
//         .map(|line| line.unwrap().trim().to_string())
//         .collect::<Vec<String>>()
//         .join(" ")
//         .to_lowercase();
//     let text = text.split(" ")
//         .collect::<Vec<&str>>();
//     let iter_len = text.len() - max_len;
//
//     println!("Iterating over all words..");
//     for i in 0..iter_len {
//         let slice = text.get(i..i + max_len + 1).unwrap().to_vec();
//         if let Ok(result) = tree.traverse(VecDeque::from(slice.clone())) {
//             println!("Default: '{}' -> {:?}", slice.clone().join(" "), result);
//         }
//
//         let sres = symspell.lookup_compound(text.get(i..i + max_len + 1).unwrap().join(" ").as_str(), 2);
//         let sslice = sres[0].term.split(" ").collect::<Vec<&str>>();
//         if let Ok(result) = tree.traverse(VecDeque::from(sslice.clone())) {
//             println!("SymSpell: '{}' -> '{}' -> {:?}", slice.join(" "), sslice.join(" "), result);
//         }
//     }
// }