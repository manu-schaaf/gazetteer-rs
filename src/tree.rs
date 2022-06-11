use std::cmp::Ordering;
use std::collections::vec_deque::VecDeque;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use ngrams::Ngram;
use rayon::prelude::*;
#[cfg(feature = "server")]
use rocket::FromFormField;
#[cfg(feature = "server")]
use rocket::serde::{Deserialize, Serialize};

use crate::util::{get_files, get_spinner, parse_files, read_lines, split_with_indices};

#[cfg_attr(feature = "server", derive(Debug, FromFormField, Serialize, Deserialize))]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
pub enum ResultSelection {
    All,
    Last,
    Longest,
}

pub trait SearchTree: Sync + Send {
    fn default() -> Self
        where Self: Sized;
    fn load(root_path: &str) -> Self
        where Self: Sized;
    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &Vec<String>)>, String>;

    fn search<'a>(&self, text: &'a str, max_len: Option<usize>, result_selection: Option<&ResultSelection>) -> Vec<(String, Vec<String>, usize, usize)> {
        let result_selection = result_selection.unwrap_or(&ResultSelection::Longest);
        let max_len = max_len.unwrap_or(5 as usize);

        let (mut slices, mut offsets) = split_with_indices(text);

        // Pad the slices and their offsets to include the last words
        slices.extend(vec![""; max_len]);
        offsets.extend(vec![(0, 0); max_len]);
        let (slices, offsets) = (slices, offsets);

        let mut results = slices
            .par_windows(max_len)
            .map(|slice| self.traverse(VecDeque::from(slice.to_vec())))
            .zip(offsets.par_windows(max_len))
            .filter_map(|(result, offsets)| if let Ok(result) = result { Some((result, offsets)) } else { None })
            .filter_map(|(result, offsets)| if !result.is_empty() { Some((result, offsets)) } else { None })
            .map(|(results, offsets)| {
                let start = offsets[0].0;
                match result_selection {
                    ResultSelection::All => {
                        let mut returns = Vec::new();
                        for result in results {
                            let end = offsets[result.0.len() - 1].1;
                            returns.push((result.0.join(" "), result.1.clone(), start, end));
                        }
                        returns
                    }
                    ResultSelection::Last => {
                        let result = results.last().unwrap();
                        let end = offsets[result.0.len() - 1].1;
                        vec![(result.0.join(" "), result.1.clone(), start, end)]
                    }
                    ResultSelection::Longest => {
                        let mut result = (Vec::new(), &Vec::new());
                        for t in results {
                            if t.0.len() > result.0.len() {
                                result = t;
                            }
                        }
                        let end = offsets[result.0.len() - 1].1;
                        vec![(result.0.join(" "), result.1.clone(), start, end)]
                    }
                }
            })
            .flatten()
            .collect::<Vec<(String, Vec<String>, usize, usize)>>();

        // results.dedup_by(|b, a| b.2 <= a.3);
        // TODO: This removes fully covered entities that end on the same character as their covering entities but not partial overlaps
        results.dedup_by_key(|el| el.3);

        results
    }
}

#[derive(Debug, Clone)]
pub struct BinarySearchTree {
    pub value: String,
    pub uri: Vec<String>,
    pub children: Vec<BinarySearchTree>,
}

impl SearchTree for BinarySearchTree {
    fn default() -> Self {
        Self {
            value: "<ROOT>".to_string(),
            uri: vec![],
            children: vec![],
        }
    }

    fn load(root_path: &str) -> Self {
        let mut root = Self::default();
        let files: Vec<String> = get_files(root_path);

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template(&format!(
                "Loading Input Files {{bar:40}} {{pos}}/{{len}} {{msg}}"
            )).unwrap()
        );
        let mut lines = parse_files(files, Option::from(&pb), None);
        lines.sort_unstable();
        let lines = lines;
        pb.finish_with_message("Done");

        let pb = ProgressBar::new(lines.len() as u64);
        pb.set_style(ProgressStyle::default_bar()
            .template(&format!(
                "Building Tree {{bar:40}} {{pos}}/{{len}} {{msg}}"
            )).unwrap()
        );
        root._load_lines_in_order(lines, Option::from(&pb));
        pb.finish_with_message("Done");

        root
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &Vec<String>)>, String> {
        let vec = self._traverse(values, Vec::new(), Vec::new());
        if vec.len() > 0 {
            Ok(vec)
        } else {
            Err(String::from("No matches found"))
        }
    }
}


impl BinarySearchTree {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            value: "<ROOT>".to_string(),
            uri: vec![],
            children: Vec::with_capacity(capacity),
        }
    }

    fn from(value: &str, uri: String) -> Self {
        let value = String::from(value);
        Self {
            value,
            uri: vec![uri],
            children: vec![],
        }
    }

    fn child(value: &str) -> Self {
        let value = String::from(value);
        Self {
            value,
            uri: vec![],
            children: vec![],
        }
    }

    fn get_value(&self) -> &String {
        &self.value
    }

    pub fn insert(&mut self, mut values: VecDeque<&str>, uri: String) {
        let value = &values.pop_front().unwrap().to_lowercase();
        match self.children.binary_search_by_key(&value, |a| a.get_value()) {
            Ok(idx) => {
                if values.is_empty() {
                    self.children[idx].uri.push(uri);
                    self.children[idx].uri.sort();
                    self.children[idx].uri.dedup();
                } else {
                    self.children[idx].insert(values, uri);
                }
            }
            Err(idx) => {
                if values.is_empty() {
                    self.children.insert(idx, BinarySearchTree::from(value, uri));
                } else {
                    self.children.insert(idx, BinarySearchTree::child(value));
                    self.children[idx].insert(values, uri);
                }
            }
        }
    }

    pub fn insert_in_order(&mut self, mut values: VecDeque<&str>, uri: String) {
        let value = &values.pop_front().unwrap().to_lowercase();
        if let Some(last_child) = self.children.last_mut() && last_child.value.eq(value) {
            if values.is_empty() {
                last_child.uri.push(uri);
                last_child.uri.sort();
                last_child.uri.dedup();
            } else {
                last_child.insert_in_order(values, uri);
            }
        } else {
            if values.is_empty() {
                self.children.push(BinarySearchTree::from(value, uri));
            } else {
                self.children.push(BinarySearchTree::child(value));
                self.children.last_mut().unwrap().insert_in_order(values, uri);
            }
        }
    }

    fn join(&mut self, other: &BinarySearchTree) {
        let children = &mut self.children;
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
        pb.finish_with_message("Done")

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

    fn _load_lines(&mut self, lines: Vec<(String, String)>, pb: Option<&ProgressBar>) {
        for (taxon_name, uri) in lines {
            self.insert(VecDeque::from(split_with_indices(&taxon_name).0), String::from(uri));

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn _load_lines_in_order(&mut self, lines: Vec<(String, String)>, pb: Option<&ProgressBar>) {
        for (taxon_name, uri) in lines {
            self.insert_in_order(VecDeque::from(split_with_indices(&taxon_name).0), String::from(uri));

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn _traverse<'a>(
        &'a self,
        mut values: VecDeque<&'a str>,
        mut matched_string_buffer: Vec<&'a str>,
        mut results: Vec<(Vec<&'a str>, &'a Vec<String>)>,
    ) -> Vec<(Vec<&'a str>, &'a Vec<String>)> {
        let value = values.pop_front().expect("");
        match self.children.binary_search_by_key(&value.to_lowercase().as_str(), |a| a.get_value()) {
            Ok(idx) => {
                matched_string_buffer.push(value);
                if !self.children[idx].uri.is_empty() {
                    results.push((matched_string_buffer.clone(), &self.children[idx].uri));
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

#[derive(Debug, Clone)]
pub struct MultiTree {
    pub value: String,
    pub uri: String,
    pub children: Vec<BinarySearchTree>,
    each_size: usize,
}

impl SearchTree for MultiTree {
    fn default() -> Self {
        Self {
            value: "<HYPER_ROOT>".to_string(),
            uri: "".to_string(),
            children: vec![],
            each_size: 500_000,
        }
    }

    fn load(root_path: &str) -> Self {
        let mut root = Self::default();
        root.add_balanced(root_path, false, false, None);
        root
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &Vec<String>)>, String> {
        let results = self.children.par_iter()
            .filter_map(|tree| tree.traverse(values.clone()).ok())
            .flatten()
            .collect::<Vec<(Vec<&str>, &Vec<String>)>>();
        if results.is_empty() {
            Err(String::from("No matches found"))
        } else {
            Ok(results)
        }
    }
}

impl MultiTree {
    fn from_taxon_list(root_path: &str, each_size: i32, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) -> Self {
        let mut root = Self {
            value: "<HYPER_ROOT>".to_string(),
            uri: "".to_string(),
            children: vec![],
            each_size: each_size as usize,
        };

        root.add_balanced(root_path, generate_ngrams, generate_abbrv, filter_list);

        root
    }

    pub fn add_balanced(&mut self, root_path: &str, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) {
        self._load_balanced(root_path, self.each_size as usize, generate_ngrams, generate_abbrv, filter_list);
    }

    fn add_taxon(&mut self, root_path: &str, filter_list: Option<&Vec<String>>) {
        self._load_balanced(root_path, self.each_size as usize, true, true, filter_list);
    }

    pub fn add_vernacular(&mut self, root_path: &str, filter_list: Option<&Vec<String>>) {
        self._load_balanced(root_path, self.each_size as usize, false, false, filter_list);
    }

    fn _load_balanced<'data>(&mut self, root_path: &str, each_size: usize, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Input Files {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let mut lines = parse_files(files, Option::from(&pb), filter_list);

        let mut additional = Vec::new();
        if generate_ngrams {
            let filtered = lines.par_iter()
                .filter(|(taxon_name, _)| split_with_indices(&taxon_name).0.len() > 2)
                .collect::<Vec<&(String, String)>>();
            let pb = ProgressBar::new(filtered.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Generating N-Grams {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            let mut ngrams = filtered.par_iter()
                .map(|(taxon_name, uri)| {
                    let mut result = Vec::new();
                    let ngrams = split_with_indices(&taxon_name).0.into_iter()
                        .ngrams(2)
                        .pad()
                        .collect::<Vec<Vec<&str>>>();
                    for ngram in ngrams {
                        // Check whether any part is an abbreviation
                        if ngram.iter().all(|el| el.len() > 2) {
                            result.push((ngram.join(" "), String::from(uri)));
                        }
                    }
                    pb.inc(1);
                    result
                })
                .flatten()
                .collect::<Vec<(String, String)>>();

            pb.finish_with_message(format!("Adding {} n-grams", ngrams.len()));
            additional.append(&mut ngrams);
        }

        if generate_abbrv {
            let filtered = lines.par_iter()
                .filter(|(taxon_name, _)| taxon_name.split(" ").collect::<Vec<_>>().len() > 1)
                .collect::<Vec<&(String, String)>>();
            let pb = ProgressBar::new(filtered.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Generating Abbreviations {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            let mut abbrevations = filtered.par_iter()
                .map(|(taxon_name, uri)| {
                    let mut result = Vec::new();

                    let string = taxon_name.clone();
                    let clone = string.split(" ").collect::<Vec<_>>();
                    let head = String::from(clone[0]);
                    let first_char = head.chars().next().unwrap().to_string();
                    let mut abbrv = vec![first_char.as_str()];
                    abbrv.extend_from_slice(&clone[1..]);
                    result.push((abbrv.join(" "), String::from(uri)));

                    pb.inc(1);
                    result
                })
                .flatten()
                .collect::<Vec<(String, String)>>();

            pb.finish_with_message("Done");
            additional.append(&mut abbrevations);
        }
        lines.append(&mut additional);

        let pb = get_spinner();
        pb.set_message(format!("Sorting {} lines..", lines.len()));
        lines.sort_unstable();
        pb.finish();
        let pb = get_spinner();
        pb.set_message(format!("Dropping duplicates.."));
        lines.dedup();
        pb.finish();
        let lines = lines;

        let mut start_end: Vec<(usize, usize)> = Vec::new();
        for start in (0..lines.len()).step_by(each_size) {
            let size = usize::min(start + each_size, lines.len());
            start_end.push((start, size));
        }

        let mp = MultiProgress::new();
        let mut tasks: Vec<(&[(String, String)], ProgressBar)> = Vec::new();
        for (start, end) in start_end {
            let pb = mp.add(ProgressBar::new((end - start) as u64));
            pb.set_style(ProgressStyle::with_template(&format!(
                "Building Split {:>2}/{} {{bar:40}} {{pos}}/{{len}} {{msg}}",
                end / each_size,
                lines.len() / each_size
            )).unwrap());
            tasks.push((&lines[start..end], pb));
        }

        let mut results = tasks.par_iter()
            .map(|(lines, pb)| {
                let mut tree = BinarySearchTree::with_capacity(each_size);
                tree._load_lines_in_order(Vec::from(*lines), Option::from(pb));
                pb.finish();
                tree
            }).collect::<Vec<BinarySearchTree>>();
        self.children.append(&mut results);
    }
}

fn process_test_file(tree: &impl SearchTree, max_len: Option<i32>) {
    let max_len = max_len.or(Option::from(5)).unwrap() as usize;

    println!("Loading test file..");
    let text = read_lines("resources/216578.txt")
        .join(" ");

    process_test_output(tree.search(&text, Option::from(max_len), Option::from(&ResultSelection::Last)));
}

fn process_test_output(results: Vec<(String, Vec<String>, usize, usize)>) {
    for result in results {
        println!("{:?} ({},{}) -> {:}", result.0, result.2, result.3, result.1.join("; "))
    }
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
    let mut filter_list = read_lines("resources/filter_de.txt");
    filter_list.sort_unstable();

    let mut tree = MultiTree::default();
    tree.add_balanced("resources/taxa/_current/taxon/*.list", false, true, Option::from(&filter_list));
    tree.add_vernacular("resources/taxa/_current/vernacular/*.list", Option::from(&filter_list));
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