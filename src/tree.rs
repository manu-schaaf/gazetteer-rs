use std::collections::{HashMap, HashSet};
use std::collections::vec_deque::VecDeque;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use ngrams::Ngram;
use rayon::prelude::*;
#[cfg(feature = "server")]
use rocket::form::FromFormField;
#[cfg(feature = "server")]
use rocket::serde::{Deserialize, Serialize};

use crate::util::{get_files, parse_files, Tokenizer};

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
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum MatchType {
    None,
    Full,
    Abbreviated,
    NGram,
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

#[cfg_attr(feature = "server", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "server", serde(crate = "rocket::serde"))]
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Match {
    match_type: MatchType,
    match_string: String,
    match_uri: String,
}

impl Hash for Match {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.match_uri.hash(state);
    }
}

impl Display for Match {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write! {f, "{} Match: {} -> {}", self.match_type, self.match_string, self.match_uri}
    }
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
}


pub trait SearchTree: Sync + Send {
    fn default() -> Self
        where Self: Sized;

    fn tokenize(&self, input: &str) -> Result<(Vec<String>, Vec<(usize, usize)>), String>;

    fn tokenize_batch(&self, input: &[String]) -> Result<Vec<(Vec<String>, Vec<(usize, usize)>)>, String>;

    fn load(&mut self, root_path: &str, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files to read", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Input Files {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let lines = parse_files(files, Option::from(&pb), filter_list);

        let (taxa, _, _): (Vec<String>, Vec<String>, Vec<MatchType>) = lines.clone().into_iter().multiunzip();
        let segmented: Vec<(Vec<String>, Vec<(usize, usize)>)> = self.tokenize_batch(taxa.as_slice()).unwrap();
        let entries = segmented.into_iter().zip(lines.into_iter())
            .map(|(segments, (taxon, uri, match_type))| (segments.0, taxon.clone(), uri.clone(), match_type.clone()))
            .collect::<Vec<(Vec<String>, String, String, MatchType)>>();

        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Entries {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        self.load_entries(entries.clone(), Some(&pb));
        pb.finish_with_message("Done");

        if generate_ngrams {
            let ngrams = Self::generate_ngrams(&entries);

            let pb = ProgressBar::new(entries.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Loading n-Grams {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            self.load_entries(ngrams, Some(&pb));
            pb.finish_with_message("Done");
        }

        if generate_abbrv {
            let abbreviations = Self::generate_abbreviations(&entries);

            let pb = ProgressBar::new(entries.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Loading Abbreviations {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            self.load_entries(abbreviations, Some(&pb));
            pb.finish_with_message("Done");
        }
    }

    fn generate_ngrams(lines: &Vec<(Vec<String>, String, String, MatchType)>) -> Vec<(Vec<String>, String, String, MatchType)> {
        let filtered = lines.par_iter()
            .filter(|(segments, _, _, _)| segments.len() > 2)
            .collect::<Vec<_>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Generating N-Grams {bar:40} {pos}/{len} {msg}"
        ).unwrap());

        let ngrams = filtered.par_iter()
            .map(|(segments, taxon_name, uri, _)| {
                let mut result = Vec::new();
                let ngrams = segments.clone().into_iter()
                    .ngrams(2)
                    .pad()
                    .collect::<Vec<Vec<String>>>();
                for ngram in ngrams {
                    // Check whether any part is an abbreviation
                    if ngram.iter().all(|el| el.len() > 2) {
                        result.push((ngram, String::from(taxon_name), String::from(uri), MatchType::NGram));
                    }
                }
                pb.inc(1);
                result
            })
            .flatten()
            .collect::<Vec<(Vec<String>, String, String, MatchType)>>();

        pb.finish_with_message(format!("Adding {} n-grams", ngrams.len()));
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
            .map(|(segments, taxon_name, uri, _)| {
                let mut result = Vec::new();

                let head = String::from(&segments[0]);
                let first_char = head.chars().next().unwrap().to_string();
                let mut abbrv = vec![first_char];
                abbrv.extend_from_slice(&segments[1..]);
                result.push((abbrv, String::from(taxon_name), String::from(uri), MatchType::Abbreviated));

                pb.inc(1);
                result
            })
            .flatten()
            .collect::<Vec<(Vec<String>, String, String, MatchType)>>();

        pb.finish_with_message("Done");
        abbrevations
    }

    fn load_entries(&mut self, entries: Vec<(Vec<String>, String, String, MatchType)>, pb: Option<&ProgressBar>);

    fn traverse(&self, values: VecDeque<String>) -> Result<Vec<(Vec<String>, &HashSet<Match>)>, String> {
        let vec = self.traverse_internal(values, Vec::new(), Vec::new());
        if vec.len() > 0 {
            Ok(vec)
        } else {
            Err(String::from("No matches found"))
        }
    }

    fn traverse_internal<'a>(
        &'a self,
        values: VecDeque<String>,
        matched_string_buffer: Vec<String>,
        results: Vec<(Vec<String>, &'a HashSet<Match>)>,
    ) -> Vec<(Vec<String>, &'a HashSet<Match>)>;

    fn search<'a>(&self, text: &'a str, max_len: Option<usize>, result_selection: Option<&ResultSelection>) -> Vec<(String, HashSet<Match>, usize, usize)> {
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
            .collect::<Vec<(String, HashSet<Match>, usize, usize)>>();

        // results.dedup_by(|b, a| b.2 <= a.3);
        // TODO: This removes fully covered entities that end on the same character as their covering entities but not partial overlaps
        results.dedup_by_key(|el| el.3);

        results
    }
}

#[derive(Debug)]
pub struct HashMapSearchTree {
    pub matches: HashSet<Match>,
    pub children: HashMap<String, HashMapSearchTree>,
    tokenizer: Option<Tokenizer>,
}

impl SearchTree for HashMapSearchTree {
    fn default() -> Self where Self: Sized {
        HashMapSearchTree {
            matches: HashSet::new(),
            children: HashMap::new(),
            tokenizer: Option::from(Tokenizer::default()),
        }
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

    fn tokenize_batch(&self, input: &[String]) -> Result<Vec<(Vec<String>, Vec<(usize, usize)>)>, String> {
        match &self.tokenizer {
            Some(tokenizer) => {
                Ok(tokenizer.encode_batch(input))
            }
            None => {
                Err(String::from("Missing tokenizer!"))
            }
        }
    }

    fn load_entries(&mut self, entries: Vec<(Vec<String>, String, String, MatchType)>, pb: Option<&ProgressBar>) {
        for (segments, taxon, uri, match_type) in entries {
            self.insert(VecDeque::from(segments), taxon, uri, match_type);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
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

impl HashMapSearchTree {
    fn from(match_string: String, match_uri: String) -> Self {
        Self {
            matches: HashSet::from([Match::full(match_string, match_uri)]),
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

    pub fn insert(&mut self, mut values: VecDeque<String>, match_string: String, match_uri: String, match_type: MatchType) {
        let value = values.pop_front().unwrap().to_lowercase();
        match self.children.get_mut(&value.to_lowercase()) {
            Some(mut child) => {
                if values.is_empty() {
                    child.matches.insert(Match { match_type, match_string, match_uri });
                } else {
                    child.insert(values, match_string, match_uri, match_type);
                }
            }
            None => {
                if values.is_empty() {
                    self.children.insert(value, HashMapSearchTree::from(match_string, match_uri));
                } else {
                    match self.children.try_insert(value, HashMapSearchTree::child()) {
                        Ok(child) => { child.insert(values, match_string, match_uri, match_type); }
                        Err(err) => { panic!("{:?}", err) }
                    }
                }
            }
        }
    }
}