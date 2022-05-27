use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::iter::Zip;
use std::path::Path;

use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use ngrams::Ngrams;
use rayon::prelude::*;
use rocket::form::validate::len;
use rocket::futures::StreamExt;
use rocket::http::ext::IntoCollection;
use rocket::State;
use symspell::{DistanceAlgorithm, SymSpell, SymSpellBuilder, UnicodeiStringStrategy, Verbosity};
use walkdir::{DirEntry, WalkDir};

use crate::{SpellingEngine, util};
use crate::util::{load, load_symspell, read_lines};

#[derive(Default)]
pub struct StringTree {
    value: String,
    uri: String,
    children: Vec<StringTree>,
}


impl StringTree {
    pub fn root() -> Self {
        Self {
            value: "<ROOT>".to_string(),
            uri: "".to_string(),
            children: vec![],
        }
    }

    pub fn create(value: &str, uri: String) -> Self {
        let value = String::from(value);
        Self {
            value,
            uri,
            children: vec![],
        }
    }

    fn get_value(&self) -> &String {
        &self.value
    }

    pub fn insert(&mut self, mut values: VecDeque<&str>, uri: String) -> bool {
        let value = values.pop_front().unwrap();
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
                    self.children.insert(idx, StringTree::create(value, uri));
                    true
                } else {
                    self.children.insert(idx, StringTree::create(value, String::new()));
                    self.children[idx].insert(values, uri)
                }
            }
        }
    }

    pub fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(&'a String, Vec<&'a str>)>, String> {
        let vec = self._traverse(values, Vec::new(), Vec::new());
        if vec.len() > 0 {
            Ok(vec)
        } else {
            Err(String::from("No matches found"))
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


#[test]
fn test_sample() {
    let mut tree = StringTree::root();
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

fn addr_of(s: &str) -> usize {
    s.as_ptr() as usize
}

fn split_with_indices(s: &str) -> (Vec<(usize, usize)>, Vec<&str>) {
    let indices = s.match_indices(&[' ', ',', '.', ':', ':', '"', '(', ')']).collect::<Vec<_>>();

    let mut last = 0;
    let mut offsets: Vec<((usize, usize))> = Vec::new();
    let mut slices: Vec<(&str)> = Vec::new();
    for (idx, mtch) in indices {
        let slice = &s[last..idx];
        offsets.push((last.clone(), last + slice.len()));
        slices.push(slice);
        last = idx + mtch.len();
    }

    (offsets, slices)
}

#[test]
fn test_small_taxa() {
    let max_len = 5;
    let tree = load("resources/taxa/".to_string());

    println!("Loading test file..");
    let text = read_lines("resources/216578.txt").unwrap()
        .map(|line| line.unwrap().trim().to_string())
        .collect::<Vec<String>>()
        .join(" ");
    let (offsets, slices) = split_with_indices(&text);

    println!("Iterating over all words..");
    let results: Vec<Result<Vec<_>, _>> = slices.par_windows(max_len)
        .map(|slice| tree.traverse(VecDeque::from(slice.to_vec())))
        .collect();

    offsets.windows(max_len).into_iter().zip(results.into_iter()).for_each(
        |(offsets, results)| if let Ok(results) = results {
            let start = offsets[0].0;
            for result in results {
                let end = offsets[result.1.len() - 1].1;
                println!("{:?} ({},{}) -> {:}", result.1.join(" "), start, end, result.0)
            }
        }
    )
    // {
    //     if let Ok(result) = tree.traverse(VecDeque::from(slice.clone())) {
    //         println!("Default: '{}' -> {:?}", slice.clone().join(" "), result);
    //     }
    // }
}

#[test]
fn test_symspell_small_taxa() {
    let mut max_len = 5;
    let (tree, symspell) = load_symspell("resources/taxa.txt".to_string(), "resources/de-100k.txt");

    println!("Loading test file..");
    let text = read_lines("resources/216578.txt").unwrap()
        .map(|line| line.unwrap().trim().to_string())
        .collect::<Vec<String>>()
        .join(" ")
        .to_lowercase();
    let text = text.split(" ")
        .collect::<Vec<&str>>();
    let iter_len = text.len() - max_len;

    println!("Iterating over all words..");
    for i in 0..iter_len {
        let slice = text.get(i..i + max_len + 1).unwrap().to_vec();
        if let Ok(result) = tree.traverse(VecDeque::from(slice.clone())) {
            println!("Default: '{}' -> {:?}", slice.clone().join(" "), result);
        }

        let sres = symspell.lookup_compound(text.get(i..i + max_len + 1).unwrap().join(" ").as_str(), 2);
        let sslice = sres[0].term.split(" ").collect::<Vec<&str>>();
        if let Ok(result) = tree.traverse(VecDeque::from(sslice.clone())) {
            println!("SymSpell: '{}' -> '{}' -> {:?}", slice.join(" "), sslice.join(" "), result);
        }
    }
}