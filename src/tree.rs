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

#[cfg_attr(feature = "server", derive(FromFormField, Serialize, Deserialize))]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
#[derive(Debug)]
pub enum ResultSelection {
    All,
    Last,
    Longest,
}

#[cfg_attr(feature = "server", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
#[derive(Debug, Clone)]
pub enum MatchType {
    None,
    Full,
    Partial,
    Corrected,
}

#[cfg_attr(feature = "server", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
#[derive(Debug, Clone)]
pub struct Match {
    match_type: MatchType,
    match_string: String,
    match_uri: String,
}

impl Match {
    fn none() -> Self {
        Self {
            match_type: MatchType::None,
            match_string: String::new(),
            match_uri: String::new(),
        }
    }

    fn full(
        match_string: String,
        match_uri: String,
    ) -> Self {
        Match {
            match_type: MatchType::Full,
            match_string,
            match_uri,
        }
    }

    fn partial(
        match_string: String,
        match_uri: String,
    ) -> Self {
        Match {
            match_type: MatchType::Partial,
            match_string,
            match_uri,
        }
    }

    fn corrected(
        match_string: String,
        match_uri: String,
    ) -> Self {
        Match {
            match_type: MatchType::Corrected,
            match_string,
            match_uri,
        }
    }
}


pub trait SearchTree: Sync + Send {
    fn default() -> Self
        where Self: Sized;

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &Vec<Match>)>, String>;

    fn search<'a>(&self, text: &'a str, max_len: Option<usize>, result_selection: Option<&ResultSelection>) -> Vec<(String, Vec<Match>, usize, usize)> {
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
            .collect::<Vec<(String, Vec<Match>, usize, usize)>>();

        // results.dedup_by(|b, a| b.2 <= a.3);
        // TODO: This removes fully covered entities that end on the same character as their covering entities but not partial overlaps
        results.dedup_by_key(|el| el.3);

        results
    }
}

#[derive(Debug, Clone)]
pub struct BinarySearchTree {
    pub value: String,
    pub matches: Vec<Match>,
    pub children: Vec<BinarySearchTree>,
}

impl SearchTree for BinarySearchTree {
    fn default() -> Self {
        Self {
            value: "<ROOT>".to_string(),
            matches: vec![],
            children: vec![],
        }
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &Vec<Match>)>, String> {
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
            matches: vec![],
            children: Vec::with_capacity(capacity),
        }
    }

    fn from(value: &str, match_string: String, match_uri: String) -> Self {
        let value = String::from(value);
        Self {
            value,
            matches: vec![Match::full(match_string, match_uri)],
            children: vec![],
        }
    }

    fn child(value: &str) -> Self {
        let value = String::from(value);
        Self {
            value,
            matches: vec![],
            children: vec![],
        }
    }

    fn get_value(&self) -> &String {
        &self.value
    }

    pub fn insert(&mut self, mut values: VecDeque<&str>, match_string: String, match_uri: String, partial_matches: bool) {
        let value = &values.pop_front().unwrap().to_lowercase();
        match self.children.binary_search_by_key(&value, |a| a.get_value()) {
            Ok(idx) => {
                if values.is_empty() {
                    self.children[idx]._push_match(Match::full(match_string, match_uri));
                } else {
                    if partial_matches {
                        self.children[idx]._push_match(Match::partial(match_string.clone(), match_uri.clone()));
                    }
                    self.children[idx].insert(values, match_string, match_uri, false);
                }
            }
            Err(idx) => {
                if values.is_empty() {
                    self.children.insert(idx, BinarySearchTree::from(value, match_string, match_uri));
                } else {
                    self.children.insert(idx, BinarySearchTree::child(value));
                    self.children[idx].insert(values, match_string, match_uri, false);
                }
            }
        }
    }

    pub fn _push_match(&mut self, mtch: Match) {
        match self.matches.binary_search_by_key(&&mtch.match_uri, |el| &el.match_uri) {
            Ok(_) => {
                // This should not happen, unless there is a duplicate in the input data
                // or the same (match_string, match_uri) pair occurs with a different MatchType
                // TODO: Choose behavior on collision with different MatchType
            }
            Err(i) => {
                self.matches.insert(i, mtch);
            }
        }
    }

    pub fn insert_in_order(&mut self, mut values: VecDeque<&str>, match_string: String, match_uri: String, partial_matches: bool) {
        let value = &values.pop_front().unwrap().to_lowercase();
        if let Some(last_child) = self.children.last_mut() && last_child.value.eq(value) {
            if values.is_empty() {
                last_child._push_match(Match::full(match_string, match_uri));
            } else {
                if partial_matches {
                    last_child._push_match(Match::partial(match_string.clone(), match_uri.clone()));
                }
                last_child.insert_in_order(values, match_string, match_uri, partial_matches);
            }
        } else {
            if values.is_empty() {
                self.children.push(BinarySearchTree::from(value, match_string, match_uri));
            } else {
                self.children.push(BinarySearchTree::child(value));
                self.children.last_mut().unwrap().insert_in_order(values, match_string, match_uri, partial_matches);
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

    fn _load_lines(&mut self, lines: Vec<(String, String)>, pb: Option<&ProgressBar>, partial_matches: bool) {
        for (taxon_name, uri) in lines {
            self.insert(VecDeque::from(split_with_indices(&taxon_name.clone()).0), taxon_name, uri, partial_matches);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn _load_lines_in_order(&mut self, lines: Vec<(String, String)>, pb: Option<&ProgressBar>, partial_matches: bool) {
        for (taxon_name, uri) in lines {
            self.insert_in_order(VecDeque::from(split_with_indices(&taxon_name.clone()).0), taxon_name, uri, partial_matches);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn _traverse<'a>(
        &'a self,
        mut values: VecDeque<&'a str>,
        mut matched_string_buffer: Vec<&'a str>,
        mut results: Vec<(Vec<&'a str>, &'a Vec<Match>)>,
    ) -> Vec<(Vec<&'a str>, &'a Vec<Match>)> {
        let value = values.pop_front().expect("");
        match self.children.binary_search_by_key(&value.to_lowercase().as_str(), |a| a.get_value()) {
            Ok(idx) => {
                matched_string_buffer.push(value);
                if !self.children[idx].matches.is_empty() {
                    results.push((matched_string_buffer.clone(), &self.children[idx].matches));
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
    pub children: Vec<BinarySearchTree>,
    each_size: usize,
}

impl SearchTree for MultiTree {
    fn default() -> Self {
        Self {
            children: vec![],
            each_size: 500_000,
        }
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &Vec<Match>)>, String> {
        let results = self.children.par_iter()
            .filter_map(|tree| tree.traverse(values.clone()).ok())
            .flatten()
            .collect::<Vec<(Vec<&str>, &Vec<Match>)>>();
        if results.is_empty() {
            Err(String::from("No matches found"))
        } else {
            Ok(results)
        }
    }
}

impl MultiTree {
    fn load(root_path: &str) -> Self {
        let mut root = Self::default();
        root.add_balanced(root_path, false, false, false, None);
        root
    }

    fn from_taxon_list(root_path: &str, each_size: i32, generate_ngrams: bool, generate_abbrv: bool, partial_matches: bool, filter_list: Option<&Vec<String>>) -> Self {
        let mut root = Self {
            children: vec![],
            each_size: each_size as usize,
        };

        root.add_balanced(root_path, generate_ngrams, generate_abbrv, partial_matches, filter_list);

        root
    }

    pub fn add_balanced(&mut self, root_path: &str, generate_ngrams: bool, generate_abbrv: bool, partial_matches: bool, filter_list: Option<&Vec<String>>) {
        self._load_balanced(root_path, self.each_size as usize, generate_ngrams, generate_abbrv, partial_matches, filter_list);
    }

    fn _load_balanced<'data>(&mut self, root_path: &str, each_size: usize, generate_ngrams: bool, generate_abbrv: bool, partial_matches: bool, filter_list: Option<&Vec<String>>) {
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
                tree._load_lines_in_order(Vec::from(*lines), Option::from(pb), partial_matches);
                pb.finish();
                tree
            }).collect::<Vec<BinarySearchTree>>();
        self.children.append(&mut results);
    }
}