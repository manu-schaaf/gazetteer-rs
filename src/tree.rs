use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::collections::vec_deque::VecDeque;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use ngrams::Ngram;
use rayon::prelude::*;
use rocket::FromFormField;
use serde::{Deserialize, Serialize};

use crate::util::{get_files, parse_files, Tokenizer};

#[derive(Debug, Serialize, Deserialize, FromFormField)]
pub enum ResultSelection {
    All,
    Last,
    Longest,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum MatchType {
    None,
    Full,
    Abbreviated,
    NGram,
}

impl MatchType {
    fn get_value(&self) -> i32 {
        match self {
            MatchType::None => { -1 }
            MatchType::Full => { 0 }
            MatchType::Abbreviated => { 1 }
            MatchType::NGram => { 2 }
        }
    }
}

impl Ord for MatchType {
    fn cmp(&self, other: &Self) -> Ordering {
        self.get_value().cmp(&other.get_value())
    }
}

impl PartialOrd<Self> for MatchType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for MatchType {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchType::None => { write!(f, "None") }
            MatchType::Full => { write!(f, "Full") }
            MatchType::Abbreviated => { write!(f, "Abbreviated") }
            MatchType::NGram => { write!(f, "NGram") }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Match {
    pub match_type: MatchType,
    pub match_string: String,
    pub match_label: String,
}

impl Hash for Match {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.match_label.hash(state);
    }
}

impl Ord for Match {
    fn cmp(&self, other: &Self) -> Ordering {
        self.match_type.cmp(&other.match_type)
            .then(self.match_string.cmp(&other.match_string))
            .then(self.match_label.cmp(&other.match_label))
    }
}

impl PartialOrd<Self> for Match {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Display for Match {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write! {f, "{} Match: {} -> {}", self.match_type, self.match_string, self.match_label}
    }
}

impl Match {
    fn none() -> Self {
        Self {
            match_type: MatchType::None,
            match_string: String::new(),
            match_label: String::new(),
        }
    }

    fn full(
        match_string: String,
        match_label: String,
    ) -> Self {
        Match {
            match_type: MatchType::Full,
            match_string,
            match_label,
        }
    }
}


#[derive(Debug)]
pub struct HashMapSearchTree {
    pub matches: HashSet<Match>,
    pub children: HashMap<String, HashMapSearchTree>,
    tokenizer: Option<Tokenizer>,
}

impl HashMapSearchTree {
    pub fn default() -> Self where Self: Sized {
        HashMapSearchTree {
            matches: HashSet::new(),
            children: HashMap::new(),
            tokenizer: Option::from(Tokenizer::default()),
        }
    }

    fn from(match_string: String, match_label: String) -> Self {
        Self {
            matches: HashSet::from([Match::full(match_string, match_label)]),
            children: HashMap::new(),
            tokenizer: None,
        }
    }

    fn child() -> Self {
        HashMapSearchTree {
            tokenizer: None,
            ..HashMapSearchTree::default()
        }
    }

    pub fn load(&mut self, root_path: &str, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files to read", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Input Files {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let lines = parse_files(files, Option::from(&pb), filter_list);

        let search_terms: Vec<&str> = lines.iter().map(|line| line.0.as_str()).collect();
        let segmented: Vec<(Vec<String>, Vec<(usize, usize)>)> = self.tokenize_batch(search_terms.as_slice()).unwrap();
        let entries = segmented.into_iter().zip(lines.into_iter())
            .map(|(segments, (search_term, label, match_type))| (segments.0, search_term.clone(), label.clone(), match_type.clone()))
            .collect::<Vec<(Vec<String>, String, String, MatchType)>>();

        if generate_ngrams {
            let ngrams = Self::generate_ngrams(&entries);

            let pb = ProgressBar::new(ngrams.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Loading n-Grams {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            self.load_entries(ngrams, Some(&pb));
            pb.finish_with_message("Done");
        }

        if generate_abbrv {
            let abbreviations = Self::generate_abbreviations(&entries);

            let pb = ProgressBar::new(abbreviations.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Loading Abbreviations {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            self.load_entries(abbreviations, Some(&pb));
            pb.finish_with_message("Done");
        }

        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Entries {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        self.load_entries(entries, Some(&pb));
        pb.finish_with_message("Done");
    }

    fn load_entries(&mut self, entries: Vec<(Vec<String>, String, String, MatchType)>, pb: Option<&ProgressBar>) {
        for (segments, search_term, label, match_type) in entries {
            self.insert(VecDeque::from(segments), search_term, label, match_type);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    pub fn insert(&mut self, mut values: VecDeque<String>, match_string: String, match_label: String, match_type: MatchType) {
        if let Some(value) = values.pop_front() {
            let value = value.to_lowercase();
            match self.children.get_mut(&value) {
                Some(mut child) => {
                    if values.is_empty() {
                        child.matches.insert(Match { match_type, match_string, match_label });
                    } else {
                        child.insert(values, match_string, match_label, match_type);
                    }
                }
                None => {
                    if values.is_empty() {
                        self.children.insert(value, HashMapSearchTree::from(match_string, match_label));
                    } else {
                        match self.children.try_insert(value, HashMapSearchTree::child()) {
                            Ok(child) => { child.insert(values, match_string, match_label, match_type); }
                            Err(err) => { panic!("{:?}", err) }
                        }
                        // let mut child = self.children.insert(value, HashMapSearchTree::default()).expect("!");
                        // child.insert(values, match_string, match_uri, match_type);
                    }
                }
            }
        }
    }

    fn generate_ngrams(lines: &Vec<(Vec<String>, String, String, MatchType)>) -> Vec<(Vec<String>, String, String, MatchType)> {
        let filtered = lines.par_iter()
            .filter(|(segments, _, _, _)| segments.len() > 2)
            .collect::<Vec<_>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Generating n-Grams {bar:40} {pos}/{len} {msg}"
        ).unwrap());

        let ngrams = filtered.par_iter()
            .map(|(segments, search_term, label, _)| {
                let mut result = Vec::new();
                let ngrams = segments.clone().into_iter()
                    .ngrams(2)
                    .pad()
                    .collect::<Vec<Vec<String>>>();
                for ngram in ngrams {
                    // Check whether any part is an abbreviation
                    if ngram.iter().all(|el| el.len() > 2) {
                        result.push((ngram, String::from(search_term), String::from(label), MatchType::NGram));
                    }
                }
                pb.inc(1);
                result
            })
            .flatten()
            .collect::<Vec<(Vec<String>, String, String, MatchType)>>();

        pb.finish_with_message(format!("Generated {} n-grams", ngrams.len()));
        ngrams
    }

    fn generate_abbreviations(lines: &Vec<(Vec<String>, String, String, MatchType)>) -> Vec<(Vec<String>, String, String, MatchType)> {
        let filtered = lines.par_iter()
            .filter(|(segments, _, _, _)| segments.len() > 1)
            .collect::<Vec<_>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Generating Abbreviations {bar:40} {pos}/{len} {msg}"
        ).unwrap());

        let abbrevations = filtered.par_iter()
            .map(|(segments, search_term, label, _)| {
                let mut result = Vec::new();

                let head = String::from(&segments[0]);
                let first_char = head.chars().next().unwrap().to_string();
                let mut abbrv = vec![first_char];
                abbrv.extend_from_slice(&segments[1..]);
                result.push((abbrv, String::from(search_term), String::from(label), MatchType::Abbreviated));

                pb.inc(1);
                result
            })
            .flatten()
            .collect::<Vec<(Vec<String>, String, String, MatchType)>>();

        pb.finish_with_message("Done");
        abbrevations
    }

    fn tokenize(&self, input: &str) -> Result<(Vec<String>, Vec<(usize, usize)>), String> {
        match &self.tokenizer {
            Some(tokenizer) => {
                Ok(tokenizer.tokenize(input))
            }
            None => {
                Err(String::from("Missing tokenizer!"))
            }
        }
    }

    fn tokenize_batch(&self, input: &[&str]) -> Result<Vec<(Vec<String>, Vec<(usize, usize)>)>, String> {
        match &self.tokenizer {
            Some(tokenizer) => {
                Ok(tokenizer.encode_batch(input))
            }
            None => {
                Err(String::from("Missing tokenizer!"))
            }
        }
    }

    fn traverse(&self, values: VecDeque<String>) -> Result<Vec<(Vec<String>, &HashSet<Match>)>, String> {
        let vec = self.traverse_internal(values, Vec::new(), Vec::new());
        if vec.len() > 0 {
            Ok(vec)
        } else {
            Err(String::from("No matches found"))
        }
    }

    pub fn search<'a>(&self, text: &'a str, max_len: Option<usize>, result_selection: Option<&ResultSelection>) -> Vec<(String, Vec<Match>, usize, usize)> {
        let result_selection = result_selection.unwrap_or(&ResultSelection::Longest);
        let max_len = max_len.unwrap_or(5 as usize);

        let (mut slices, mut offsets) = self.tokenize(text).unwrap();

        // Pad the slices and their offsets to include the last words
        slices.extend(vec![String::new(); max_len]);
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
                        let mut result = (Vec::new(), &HashSet::new());
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
            .map(|(s, mtches, a, b)| (s, mtches.into_iter().sorted().collect::<Vec<Match>>(), a, b))
            .collect::<Vec<(String, Vec<Match>, usize, usize)>>();

        // results.dedup_by(|b, a| b.2 <= a.3);
        // TODO: This removes fully covered entities that end on the same character as their covering entities but not partial overlaps
        results.dedup_by_key(|el| el.3);

        results
    }

    fn traverse_internal<'a>(
        &'a self,
        mut values: VecDeque<String>,
        mut matched_string_buffer: Vec<String>,
        mut results: Vec<(Vec<String>, &'a HashSet<Match>)>,
    ) -> Vec<(Vec<String>, &'a HashSet<Match>)> {
        let value = values.pop_front().expect("");
        match self.children.get(&value.to_lowercase()) {
            Some(child) => {
                matched_string_buffer.push(value.to_string());
                if !child.matches.is_empty() {
                    results.push((matched_string_buffer.clone(), &child.matches));
                }

                if !values.is_empty() {
                    child.traverse_internal(values, matched_string_buffer, results)
                } else {
                    results
                }
            }
            None => {
                results
            }
        }
    }
}