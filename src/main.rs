#![feature(slice_take)]
#![feature(let_chains)]
#[macro_use]
extern crate rocket;
extern crate symspell;

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use std::string::String;

use indicatif::ProgressBar;
use ngrams::Ngrams;
use rocket::{Build, Rocket, State};
use symspell::{DistanceAlgorithm, SymSpell, SymSpellBuilder, UnicodeiStringStrategy, Verbosity};

use tree::StringTree;

mod tree;
mod util;

struct SearchTree {
    tree: StringTree,
}

struct SpellingEngine {
    engine: SymSpell<UnicodeiStringStrategy>,
}

// #[get("/")]
// fn index(symspell: &State<SpellingEngine>) -> String {
//     let sentence = "ranuncalus auriconus"; // ranunculus auricomus
//     let compound_suggestions = symspell.engine.lookup_compound(sentence, 2);
//     format!("{:?}", compound_suggestions)
// }
//
// #[launch]
// fn rocket() -> Rocket<Build> {
//     // println!("Loading SymSpell dictionary..");
//     // let mut symspell: SymSpell<UnicodeiStringStrategy> = SymSpellBuilder::default()
//     //     .max_dictionary_edit_distance(2)
//     //     .count_threshold(0)
//     //     .distance_algorithm(DistanceAlgorithm::SIMD)
//     //     .build()
//     //     .unwrap();
//     //
//     // symspell.load_dictionary("resources/word_count.txt", 0, 1, " ");
//     // symspell.load_bigram_dictionary(
//     //     "resources/bigram_count.txt",
//     //     0,
//     //     2,
//     //     " "
//     // );
//     let (tree, symspell) = tree::load();
//
//     rocket::build()
//         .mount("/", routes![index])
//         .manage(SpellingEngine { engine: symspell })
//         .manage(SearchTree { tree: tree })
// }

fn main() {
    println!("Hello World")
    // let (tree, symspell) = util::load_symspell("resources/taxa/Lichen/".to_string(), "resources/de-100k.txt");
    // let string = String::from("Lyronna dolichobellum abc abc").to_lowercase();
    // println!("{:?}", tree.traverse(string.clone().split(' ').collect::<VecDeque<&str>>()));
    // let results = symspell.lookup_compound(string.as_str(), 2);
    // if results.len() > 0 {
    //     println!("{}", results[0].term);
    //     println!("{:?}", tree.traverse(results[0].term.split(' ').collect::<VecDeque<&str>>()));
    // }
}

#[test]
fn test_all() {
    let mut symspell: SymSpell<UnicodeiStringStrategy> = SymSpellBuilder::default()
        .max_dictionary_edit_distance(2)
        .count_threshold(0)
        .distance_algorithm(DistanceAlgorithm::SIMD)
        .build()
        .unwrap();

    symspell.load_dictionary("resources/word_count.txt", 0, 1, " ");
    symspell.load_bigram_dictionary(
        "resources/bigram_count.txt",
        0,
        2,
        " ",
    );

    let sentence = "americona"; // americana
    let suggestions = symspell.lookup(sentence, Verbosity::Top, 2);
    println!("{:?}", suggestions);

    let sentence = "ranuncalus auriconus"; // ranunculus auricomus
    let compound_suggestions = symspell.lookup_compound(sentence, 2);
    println!("{:?}", compound_suggestions);

    let sentence = "Sauropus androgynus (Syn.: Breynia androgyna) ist eine Pflanzenart in der Familie der Phyllanthaceae aus Indien, Südostasien bis ins südliche China.";
    let compound_suggestions = symspell.lookup_compound(sentence, 2);
    println!("{:?}", compound_suggestions);

    let sentence = "ranunculusauricomus";
    let segmented = symspell.word_segmentation(sentence, 2);
    println!("{:?}", segmented);
}
