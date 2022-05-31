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
use tokenizers::normalizers::{NFKC, NFKD};
use tokenizers::pre_tokenizers::unicode_scripts::UnicodeScripts;
use tokenizers::{NormalizedString, Normalizer, OffsetReferential, OffsetType, PreTokenizedString, PreTokenizer, SplitDelimiterBehavior};
use tokenizers::pre_tokenizers::punctuation::Punctuation;
use tokenizers::pre_tokenizers::whitespace::Whitespace;

use gazetteer::tree::StringTree;
use gazetteer::util::read_lines;

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

    let text = read_lines("resources/216578.txt").join(" ");
    let mut text = PreTokenizedString::from(text);

    let normalizer = NFKC::default();
    let punctuation = Punctuation::new(SplitDelimiterBehavior::Removed);
    let whitespace = Whitespace::default();

    text.normalize(|s| normalizer.normalize(s));
    punctuation.pre_tokenize(&mut text);
    whitespace.pre_tokenize(&mut text);
    let vec = text.get_splits(OffsetReferential::Original, OffsetType::Char);
    for (slice, offsets, optino_token) in vec {
        println!("{:} ({}, {})", slice, offsets.0, offsets.1);
    }
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
