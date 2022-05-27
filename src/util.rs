use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::Path;

use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use itertools::{EitherOrBoth, merge_join_by};
use ngrams::Ngrams;
use rayon::prelude::*;
use symspell::{DistanceAlgorithm, SymSpell, SymSpellBuilder, UnicodeiStringStrategy};
use walkdir::WalkDir;

use crate::StringTree;

pub fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
    where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

fn get_files(root_path: String) -> Vec<String> {
    println!("Reading resources dir...");
    let mut files = WalkDir::new(root_path)
        .into_iter()
        .filter_map(|file| file.ok())
        .filter(|file| file.metadata().unwrap().is_file())
        .map(|file| String::from(file.path().to_str().unwrap()))
        .collect::<Vec<String>>();
    files.sort_by_key(|a| a.to_lowercase());
    files
}


pub fn load(root_path: String) -> StringTree {
    let mut tree = StringTree::root();
    let files = get_files(root_path);
    for file in files {
        process_file(file, &mut tree);
    }
    tree
}

pub fn load_parallel(root_path: String) -> Vec<StringTree> {
    let files = get_files(root_path);
    let trees = files.par_iter().map(
        |file| load_file(String::from(file))
    ).collect::<Vec<StringTree>>();
    // let mut trees = VecDeque::from(trees);
    // let mut tree = trees.pop_front().unwrap();
    // for other in trees {
    //     tree.join(&other);
    // }
    // tree
    trees
}

fn load_file(file: String) -> StringTree {
    let mut tree = StringTree::root();
    println!("{}", file);
    if let Ok(lines) = read_lines(Path::new(file.as_str())) {
        let lines = lines.into_iter().collect::<Vec<_>>();
        let pb = ProgressBar::new(lines.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template(&format!(
                "Loading {} [{{elapsed_precise}}] {{bar:40}} {{pos}}/{{len}} {{msg}}", file
            )).unwrap()
        );
        for line in lines {
            if let Ok(line) = line {
                let line = line.to_lowercase();
                if line.trim().len() > 0 {
                    let split = line.split('\t').collect::<Vec<&str>>();
                    let taxon_name = split[0].split(' ').collect::<Vec<&str>>();
                    let uri = split[1].to_string();
                    tree.insert(VecDeque::from(taxon_name), uri);
                }
            }
            pb.inc(1);
        }
        pb.finish_with_message("done");
    }
    tree
}

fn process_file(file: String, tree: &mut StringTree) -> &mut StringTree {
    println!("{}", file);
    if let Ok(lines) = read_lines(Path::new(file.as_str())) {
        let lines = lines.into_iter().collect::<Vec<_>>();
        let pb = ProgressBar::new(lines.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40} {pos}/{len} {msg}").unwrap()
        );
        for line in lines {
            if let Ok(line) = line {
                let line = line.to_lowercase();
                if line.trim().len() > 0 {
                    let split = line.split('\t').collect::<Vec<&str>>();
                    let taxon_name = split[0].split(' ').collect::<Vec<&str>>();
                    let uri = split[1].to_string();
                    tree.insert(VecDeque::from(taxon_name), uri);
                }
            }
            pb.inc(1);
        }
        pb.finish_with_message("done");
    }
    tree
}

pub fn load_symspell(root_path: String, additional_dictionary: &str) -> (StringTree, SymSpell<UnicodeiStringStrategy>) {
    let style: ProgressStyle = ProgressStyle::default_bar().template("[{elapsed_precise}] {bar:40} {pos}/{len} {msg}").unwrap();
    let mut tree = StringTree::root();

    let mut unigram_counter = HashMap::new();
    let mut bigram_counter = HashMap::new();

    println!("Reading resources dir...");
    let files = get_files(root_path);
    for file in files {
        process_file_and_count(file, &mut tree, &mut unigram_counter, &mut bigram_counter)
    }

    let mut symspell: SymSpell<UnicodeiStringStrategy> = SymSpellBuilder::default()
        .max_dictionary_edit_distance(2)
        .count_threshold(0)
        .distance_algorithm(DistanceAlgorithm::SIMD)
        .build()
        .unwrap();

    if let Ok(lines) = read_lines(Path::new(additional_dictionary)) {
        let lines = lines.into_iter().collect::<Vec<_>>();
        for line in lines.iter().progress_with_style(style.clone()) {
            if let Ok(line) = line {
                symspell.load_dictionary_line(line.as_str(), 0, 1, " ");
            }
        }
    }
    symspell.load_dictionary(additional_dictionary, 0, 1, " ");

    let mut vec = unigram_counter.iter().collect::<Vec<(&String, &i32)>>();
    vec.sort_by_key(|tup| tup.1);
    for (gram, count) in vec.iter().progress_with_style(style.clone()) {
        let joined = [gram, count.to_string().as_str()].join(" ");
        symspell.load_dictionary_line(joined.as_str(), 0, 1, " ");
    }

    let mut vec = bigram_counter.iter().collect::<Vec<(&String, &i32)>>();
    vec.sort_by_key(|tup| tup.1);
    for (gram, count) in vec.iter().progress_with_style(style.clone()) {
        let joined = [gram, count.to_string().as_str()].join(" ");
        symspell.load_bigram_dictionary_line(joined.as_str(), 0, 2, " ");
    }

    (tree, symspell)
}

fn process_file_and_count(file: String, tree: &mut StringTree, unigram_counter: &mut HashMap<String, i32>, bigram_counter: &mut HashMap<String, i32>) {
    println!("{}", file);
    if let Ok(lines) = read_lines(Path::new(file.as_str())) {
        let lines = lines.into_iter().collect::<Vec<_>>();
        let pb = ProgressBar::new(lines.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40} {pos}/{len} {msg}").unwrap()
        );
        for line in lines {
            if let Ok(line) = line {
                let line = line.to_lowercase();
                if line.trim().len() > 0 {
                    let split = line.split('\t').collect::<Vec<&str>>();
                    let taxon_name = split[0].split(' ').collect::<Vec<&str>>();
                    let uri = split[1].to_string();

                    for el in taxon_name.clone() {
                        *unigram_counter.entry(el.to_string()).or_insert(0) += 1;
                    }

                    for ngram in Ngrams::new(split[0].split(' '), 2).pad() {
                        let ngram = ngram.join(" ");
                        *bigram_counter.entry(ngram).or_insert(0) += 1;
                    }

                    tree.insert(VecDeque::from(taxon_name), uri);
                }
            }
            pb.inc(1);
        }
        pb.finish_with_message("done");
    }
}
