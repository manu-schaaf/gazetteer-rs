use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::iter::Zip;
use std::mem::take;
use std::path::Path;
use std::vec::IntoIter;

use indicatif::{MultiProgress, ProgressBar, ProgressIterator, ProgressStyle};
use itertools::{EitherOrBoth, merge_join_by};
use ngrams::{Ngram, Ngrams};
use rayon::iter::Map;
use rayon::prelude::*;
use rayon::slice::Iter;
use rocket::form::validate::{len, Len};
use rocket::futures::StreamExt;
use rocket::http::ext::IntoCollection;
use rocket::State;
use symspell::{DistanceAlgorithm, SymSpell, SymSpellBuilder, UnicodeiStringStrategy, Verbosity};

use crate::{SpellingEngine, util};
use crate::util::{get_files, read_lines, split_with_indices};

pub trait SearchTree: Sync + Send {
    fn default() -> Self
        where Self: Sized;
    fn load(root_path: &str) -> Self
        where Self: Sized;
    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(&'a String, Vec<&'a str>)>, String>;
}

#[derive(Default, Clone)]
pub struct FilerList {
    list: HashSet<String>,
}

impl FilerList {
    fn default() -> Self {
        let lines = read_lines("resources/filter_de.txt");
        let mut set = HashSet::new();
        for line in lines {
            set.insert(line);
        }
        Self {
            list: set
        }
    }

    fn contains(&self, el: &str) -> bool {
        self.list.contains(&el.to_lowercase())
    }
}

#[derive(Clone)]
pub struct StringTree {
    pub value: String,
    pub uri: String,
    pub children: Vec<StringTree>,
    filter_list: FilerList,
}

#[derive(Clone)]
pub struct MultiTree {
    pub value: String,
    pub uri: String,
    pub children: Vec<StringTree>,
    each_size: usize,
    filter_list: FilerList,
}

impl SearchTree for StringTree {
    fn default() -> Self {
        Self {
            value: "<ROOT>".to_string(),
            uri: "".to_string(),
            children: vec![],
            filter_list: FilerList::default(),
        }
    }

    fn load(root_path: &str) -> Self {
        let mut root = Self::default();
        let files: Vec<String> = get_files(root_path);

        for file in files {
            let lines = read_lines(file.clone());
            let pb = ProgressBar::new(lines.len() as u64);
            pb.set_style(ProgressStyle::default_bar()
                .template(&format!(
                    "Loading {} [{{duration_precise}}] {{bar:40}} {{pos}}/{{len}} {{msg}}", file
                )).unwrap()
            );
            root._load_file(&lines, Some(&pb));
            pb.finish_with_message("Done");
        }
        root
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(&'a String, Vec<&'a str>)>, String> {
        let vec = self._traverse(values, Vec::new(), Vec::new());
        if vec.len() > 0 {
            Ok(vec)
        } else {
            Err(String::from("No matches found"))
        }
    }
}


impl StringTree {
    fn from(value: &str, uri: String) -> Self {
        let value = String::from(value);
        Self {
            value,
            uri,
            children: vec![],
            filter_list: FilerList::default(),
        }
    }

    fn get_value(&self) -> &String {
        &self.value
    }

    fn insert(&mut self, mut values: VecDeque<&str>, uri: String) -> bool {
        let value = &values.pop_front().unwrap().to_lowercase();
        match self.children.binary_search_by_key(&value, |a| a.get_value()) {
            Ok(idx) => {
                if values.is_empty() {
                    if self.children[idx].uri.is_empty() {
                        self.children[idx].uri = uri;
                        true
                    } else {
                        false
                    }
                } else {
                    self.children[idx].insert(values, uri)
                }
            }
            Err(idx) => {
                if values.is_empty() {
                    self.children.insert(idx, StringTree::from(value, uri));
                    true
                } else {
                    self.children.insert(idx, StringTree::from(value, String::new()));
                    self.children[idx].insert(values, uri)
                }
            }
        }
    }

    fn insert_in_order(&mut self, mut values: VecDeque<&str>, uri: String) -> bool {
        let value = &values.pop_front().unwrap().to_lowercase();
        if let Some(last_child) = self.children.last_mut() && last_child.value.eq(value) {
            if values.is_empty() {
                if last_child.uri.is_empty() {
                    last_child.uri = uri;
                    true
                } else {
                    false
                }
            } else {
                last_child.insert_in_order(values, uri)
            }
        } else {
            if values.is_empty() {
                self.children.push(StringTree::from(value, uri));
                true
            } else {
                self.children.push(StringTree::from(value, String::new()));
                self.children.last_mut().unwrap().insert_in_order(values, uri)
            }
        }
    }

    fn join(&mut self, other: &StringTree) {
        let mut children = &mut self.children;
        let mut s_index = 0;
        let mut o_index = 0;
        let pb = ProgressBar::new(other.children.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template("Joining [{elapsed_precise}/{duration_precise}] {bar:40} {pos}/{len} {msg}").unwrap()
        );
        while o_index < other.children.len() {
            if s_index >= children.len() {
                children.push(other.children[o_index].clone());
                o_index += 1;
                pb.inc(1);
            }
            match children[s_index].value.cmp(&other.children[o_index].value) {
                Ordering::Less => {
                    children.insert(s_index, other.children[o_index].clone());
                    o_index += 1;
                    pb.inc(1);
                }
                Ordering::Greater => {
                    s_index += 1;
                }
                Ordering::Equal => {
                    children[s_index].join(&other.children[o_index]);
                    o_index += 1;
                    pb.inc(1);
                }
            }
        }
        pb.finish_with_message("done")

        // let result: Vec<EitherOrBoth<_, _>> = merge_join_by(&children, &other.children, |a, b| a.value.cmp(&b.value)).collect();
        // for el in result {
        //     match el {
        //         EitherOrBoth::Right(el) => {
        //             let idx = children.binary_search_by_key(&el.get_value(), |a| a.get_value());
        //             match idx {
        //                 Err(idx) => {
        //                     self.children.insert(idx, el.clone());
        //                 }
        //                 Ok(_) => {
        //                     panic!("Some error occurred!")
        //                 }
        //             }
        //         }
        //         EitherOrBoth::Left(el) => {
        //             continue;
        //         }
        //         _ => {
        //             el.left().unwrap().join(&el.right().unwrap());
        //         }
        //     }
        // }
    }

    fn _load_file(&mut self, mut lines: &Vec<String>, pb: Option<&ProgressBar>) {
        for line in lines {
            if line.trim().len() > 0 {
                let line = line.trim().to_lowercase();
                let split = line.split('\t').collect::<Vec<&str>>();
                let taxon_name = split[0].split(' ').collect::<Vec<&str>>();
                let uri = split[1].to_string();
                self.insert(VecDeque::from(taxon_name), uri);
            }
            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn _load_file_in_order(&mut self, lines: &Vec<String>, pb: Option<&ProgressBar>) {
        for line in lines {
            if line.trim().len() > 0 {
                let line = line.trim().to_lowercase().to_string();
                let split = line.split('\t').collect::<Vec<&str>>();
                if split.len() < 2 {
                    continue;
                }
                let taxon_name = split[0].trim().split(' ').collect::<Vec<&str>>();
                let uri = split[1].to_string();
                self.insert_in_order(VecDeque::from(taxon_name), String::from(&uri));
            }
            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn _traverse<'a>(
        &'a self,
        mut values: VecDeque<&'a str>,
        mut matched_string_buffer: Vec<&'a str>,
        mut results: Vec<(&'a String, Vec<&'a str>)>,
    ) -> Vec<(&'a String, Vec<&'a str>)> {
        let value = values.pop_front().expect("");
        match self.children.binary_search_by_key(&value.to_lowercase().as_str(), |a| a.get_value()) {
            Ok(idx) => {
                matched_string_buffer.push(value);
                if !self.children[idx].uri.is_empty() {
                    results.push((&self.children[idx].uri, matched_string_buffer.clone()));
                }

                if !values.is_empty() {
                    self.children[idx]._traverse(values, matched_string_buffer, results)
                } else {
                    results
                }
            }
            Err(_) => {
                results
            }
        }
    }
}

impl SearchTree for MultiTree {
    fn default() -> Self {
        Self {
            value: "<HYPER_ROOT>".to_string(),
            uri: "".to_string(),
            children: vec![],
            each_size: 500_000,
            filter_list: FilerList::default(),
        }
    }

    fn load(root_path: &str) -> Self {
        let mut root = Self::default();
        let files: Vec<String> = get_files(root_path);

        let mp = MultiProgress::new();
        let mut tasks = Vec::new();
        for file in &files {
            let lines = read_lines(file);
            let pb = mp.add(ProgressBar::new(lines.len() as u64));
            pb.set_style(ProgressStyle::with_template(&format!(
                "Loading {} [{{elapsed_precise}}/{{duration_precise}}] {{bar:40}} {{pos}}/{{len}} {{msg}}", file
            )).unwrap());
            tasks.push((lines, pb))
        }

        tasks.par_iter()
            .map(|(lines, pb)| {
                let mut tree = StringTree::default();
                tree._load_file(lines, Option::from(pb));
                tree
            }).collect_into_vec(&mut root.children);
        root
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(&'a String, Vec<&'a str>)>, String> {
        let results = self.children.par_iter()
            .filter_map(|tree| tree.traverse(values.clone()).ok())
            .flatten()
            .collect::<Vec<(&String, Vec<&str>)>>();
        if results.is_empty() {
            Err(String::from("No matches found"))
        } else {
            Ok(results)
        }
    }
}

impl MultiTree {
    fn load_balanced(root_path: &str, each_size: i32, generate_additional: bool) -> Self {
        let mut root = Self {
            value: "<HYPER_ROOT>".to_string(),
            uri: "".to_string(),
            children: vec![],
            each_size: each_size as usize,
            filter_list: FilerList::default()
        };

        root.add_balanced(root_path, generate_additional);

        root
    }

    fn add_balanced(&mut self, root_path: &str, generate_additional: bool) {
        self.children.append(&mut Self::_load_balanced(root_path, self.each_size as usize, generate_additional));
    }

    fn _load_balanced<'data>(root_path: &str, each_size: usize, generate_additional: bool) -> Vec<StringTree> {
        let files: Vec<String> = get_files(root_path);

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Input Files [{elapsed_precise}/{duration_precise}] {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let mut lines = files.par_iter()
            .map(|file| {
                let lines = read_lines(file);
                pb.inc(1);
                lines
            })
            .flatten()
            .map(|line| line.trim().to_string())
            .filter(|line| line.len() > 0)
            .collect::<Vec<String>>();

        if generate_additional {
            let pb = ProgressBar::new(lines.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Generating Additional [{elapsed_precise}/{duration_precise}] {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            let mut additional = lines.par_iter().map(|line| {
                let split = line.split('\t').collect::<Vec<&str>>();
                let taxon = split[0].split(' ').collect::<Vec<&str>>();
                let uri = String::from(split[1]);
                (taxon, uri)
            })
                .filter(|(taxon_name, _)| taxon_name.len() > 2)
                .map(|(taxon_name, uri)| {
                    let mut result = Vec::new();

                    // let clone = taxon_name.clone();
                    // let head = clone[0];
                    // let first_char = head.chars().next().unwrap();
                    // let abbrv = format!("{:}.", String::from(first_char));
                    // let mut abbrv = vec![abbrv.as_str()];
                    // abbrv.extend_from_slice(&clone[1..]);
                    // let abbrv = Vec::from(abbrv);
                    // result.push((abbrv, String::from(&uri)));

                    let ngrams = taxon_name.into_iter().ngrams(2).pad().collect::<Vec<Vec<&str>>>();
                    for ngram in ngrams {
                        // Check whether any part is an abbreviation
                        if ngram.iter().all(|el| el.len() > 2) {
                            result.push((ngram, String::from(&uri)));
                        }
                    }
                    pb.inc(1);
                    result
                })
                .flatten()
                .map(|(taxon, uri)| vec![taxon.join(" "), uri].join("\t"))
                .collect::<Vec<String>>();

            pb.finish_with_message(format!("Adding {} n-grams & abbreviations\n", additional.len()));
            lines.append(&mut additional);
        }

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::with_template("{spinner} {msg}")
                .unwrap()
                .tick_strings(&[
                    "▹▹▹",
                    "▸▹▹",
                    "▹▸▹",
                    "▹▹▸",
                    "▪▪▪",
                ])
        );
        pb.set_message(format!("Sorting {} lines..", lines.len()));
        lines.sort();
        pb.finish();
        let lines = lines;

        let mut start_end: Vec<(usize, usize)> = Vec::new();
        for start in (0..lines.len()).step_by(each_size) {
            let size = usize::min(start + each_size, lines.len());
            start_end.push((start, size));
        }

        let mp = MultiProgress::new();
        let mut tasks: Vec<(&[String], ProgressBar)> = Vec::new();
        for (start, end) in start_end {
            let pb = mp.add(ProgressBar::new((end - start) as u64));
            pb.set_style(ProgressStyle::with_template(&format!(
                "Loading Split {:>3}/{} [{{duration_precise}}] {{bar:40}} {{pos}}/{{len}} {{msg}}",
                end / each_size,
                lines.len() / each_size
            )).unwrap());
            tasks.push((&lines[start..end], pb));
        }

        let results = tasks.par_iter()
            .map(|(lines, pb)| {
                let mut tree = StringTree::default();
                tree._load_file_in_order(&Vec::from(*lines), Option::from(pb));
                pb.finish();
                tree
            }).collect::<Vec<StringTree>>();
        return results;
    }
}

fn process_test_file(tree: &impl SearchTree, max_len: Option<i32>) {
    let max_len = max_len.or(Option::from(5)).unwrap() as usize;

    println!("Loading test file..");
    let text = read_lines("resources/216578.txt")
        .join(" ");
    let (offsets, slices) = split_with_indices(&text);

    println!("Iterating over all words..");
    let results = slices
        .par_windows(max_len)
        .map(|slice| tree.traverse(VecDeque::from(slice.to_vec())))
        .collect::<Vec<Result<Vec<(&String, Vec<&str>)>, String>>>();

    process_test_output(max_len, offsets, results);
}

fn process_test_output(max_len: usize, offsets: Vec<(usize, usize)>, results: Vec<Result<Vec<(&String, Vec<&str>)>, String>>) {
    offsets.windows(max_len).into_iter().zip(results.into_iter()).for_each(
        |(offsets, results): (&[(usize, usize)], Result<Vec<(&String, Vec<&str>)>, _>)| {
            if let Ok(results) = results {
                if !results.is_empty() {
                    let start = offsets[0].0;
                    let result = results.last().unwrap();
                    // for result in results {
                    let end = offsets[result.1.len() - 1].1;
                    println!("{:?} ({},{}) -> {:}", result.1.join(" "), start, end, result.0)
                    // };
                }
            }
        }
    )
}


#[test]
fn test_sample() {
    let mut tree = StringTree::default();
    for (s, uri) in vec![("An example phrase", "uri:phrase"), ("An example", "uri:example")] {
        let s = String::from(s);
        let uri = String::from(uri);
        let v: VecDeque<&str> = s.split(' ').collect::<VecDeque<&str>>();
        tree.insert(v, uri);
    }
    println!("{:?}", tree.traverse(String::from("An xyz").split(' ').collect::<VecDeque<&str>>()));
    println!("{:?}", tree.traverse(String::from("An example").split(' ').collect::<VecDeque<&str>>()));
    println!("{:?}", tree.traverse(String::from("An example phrase").split(' ').collect::<VecDeque<&str>>()));
}

#[test]
fn test_small_string_tree() {
    let tree = StringTree::load("resources/taxa.txt");
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_string_tree() {
    let tree = StringTree::load("resources/BIOfid/*");
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_tree() {
    let tree = MultiTree::load("resources/BIOfid/*");
    process_test_file(&tree, Option::from(5));
}

#[test]
fn test_big_multi_balanced() {
    // let tree = MultiTree::load_balanced("resources/BIOfid/taxa_*", 500_000);
    let mut tree = MultiTree::default();
    tree.add_balanced("resources/taxa/_current/taxon/*.list", true);
    tree.add_balanced("resources/taxa/_current/vernacular/*.list", false);
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