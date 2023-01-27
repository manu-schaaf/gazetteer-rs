use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::Hash;

use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::util::{create_skip_grams, get_files, parse_files, CorpusFormat, Tokenizer};

#[derive(Debug, Serialize, Deserialize)]  // FIXME
pub enum ResultSelection {
    All,
    Last,
    LastPreferFull,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum MatchType {
    None,
    Full,
    Abbreviated,
    SkipGram,
}

impl MatchType {
    fn get_value(&self) -> i32 {
        match self {
            MatchType::None => -1,
            MatchType::Full => 0,
            MatchType::Abbreviated => 1,
            MatchType::SkipGram => 2,
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
            MatchType::None => {
                write!(f, "None")
            }
            MatchType::Full => {
                write!(f, "Full")
            }
            MatchType::Abbreviated => {
                write!(f, "Abbreviated")
            }
            MatchType::SkipGram => {
                write!(f, "SkipGram")
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Match {
    pub match_type: MatchType,
    pub match_string: String,
    pub match_label: String,
}

impl Ord for Match {
    fn cmp(&self, other: &Self) -> Ordering {
        self.match_type
            .cmp(&other.match_type)
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

#[derive(Debug)]
pub struct HashMapSearchTree {
    pub search_map: HashMap<Vec<String>, HashSet<Match>>,
    tokenizer: Tokenizer,
    tree_depth: usize,
}

impl Default for HashMapSearchTree {
    fn default() -> Self
    where
        Self: Sized,
    {
        HashMapSearchTree {
            search_map: HashMap::new(),
            tokenizer: Tokenizer::default(),
            tree_depth: 0,
        }
    }
}
impl HashMapSearchTree {
    pub fn load_file(
        &mut self,
        root_path: &str,
        generate_skip_grams: bool,
        skip_gram_min_length: i32,
        skip_gram_max_skips: i32,
        filter_list: Option<&Vec<String>>,
        generate_abbrv: bool,
        format: &Option<CorpusFormat>,
    ) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files to read", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Loading Input Files {bar:40} {pos}/{len} {msg}").unwrap(),
        );
        let lines: Vec<(String, String)> =
            parse_files(files, Option::from(&pb), format, filter_list);
        pb.finish_with_message("Done");

        self.load(
            lines,
            generate_skip_grams,
            skip_gram_min_length,
            skip_gram_max_skips,
            generate_abbrv,
        );
    }

    pub fn load(
        &mut self,
        entries: Vec<(String, String)>,
        generate_skip_grams: bool,
        skip_gram_min_length: i32,
        skip_gram_max_skips: i32,
        generate_abbrv: bool,
    ) {
        let search_terms: Vec<&str> = entries.iter().map(|line| line.0.as_str()).collect();
        let segmented: Vec<(Vec<String>, Vec<(usize, usize)>)> =
            self.tokenize_batch(search_terms.as_slice());
        let entries: Vec<(Vec<String>, String, String)> = segmented
            .into_iter()
            .zip(entries.into_iter())
            .map(|(segments, (search_term, label))| (segments.0, search_term, label))
            .collect::<Vec<(Vec<String>, String, String)>>();

        self.load_entries(&entries);

        if generate_skip_grams {
            self.generate_skip_grams(&entries, skip_gram_min_length, skip_gram_max_skips);
        }

        if generate_abbrv {
            self.generate_abbreviations(&entries);
        }
    }

    pub(crate) fn load_entries(&mut self, entries: &Vec<(Vec<String>, String, String)>) {
        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Loading Entries {bar:40} {pos}/{len} {msg}").unwrap(),
        );

        for (segments, search_term, label) in entries {
            self.insert(
                segments.clone(),
                String::from(search_term),
                String::from(label),
                MatchType::Full,
            );
            pb.inc(1)
        }
        pb.finish_with_message("Done");
    }

    pub fn insert(
        &mut self,
        segments: Vec<String>,
        match_string: String,
        match_label: String,
        match_type: MatchType,
    ) {
        if segments.len() > self.tree_depth {
            self.tree_depth = segments.len();
        }

        match self.search_map.get_mut(&segments) {
            Some(search_result) => {
                search_result.insert(Match {
                    match_type,
                    match_string,
                    match_label,
                });
            }
            None => {
                self.search_map.insert(
                    segments,
                    HashSet::from([Match {
                        match_type,
                        match_string,
                        match_label,
                    }]),
                );
            }
        }
    }

    pub(crate) fn generate_skip_grams(
        &mut self,
        lines: &Vec<(Vec<String>, String, String)>,
        min_length: i32,
        max_skips: i32,
    ) {
        let filtered = lines
            .par_iter()
            .filter(|(segments, _, _)| segments.len() > min_length as usize)
            .collect::<Vec<_>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Generating skip-grams {bar:40} {pos}/{len} {msg}")
                .unwrap(),
        );

        let mut counter: i64 = 0;
        for (segments, search_term, label) in filtered {
            let mut deletes = create_skip_grams(vec![segments.clone()], max_skips, min_length);
            deletes.sort();
            deletes.dedup();
            for skip_gram in deletes {
                self.insert(
                    skip_gram.clone(),
                    String::from(search_term),
                    String::from(label),
                    MatchType::SkipGram,
                );
                counter += 1;
            }
            pb.inc(1);
        }
        pb.finish_with_message(format!("Generated {} skip-grams", counter));
    }

    pub(crate) fn generate_abbreviations(&mut self, lines: &Vec<(Vec<String>, String, String)>) {
        let filtered = lines
            .par_iter()
            .filter(|(segments, _, _)| segments.len() > 1)
            .collect::<Vec<_>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Generating Abbreviations {bar:40} {pos}/{len} {msg}")
                .unwrap(),
        );

        let mut counter: i64 = 0;
        let mut abbrv: Vec<String> = Vec::new();
        for (segments, search_term, label) in filtered {
            for i in 0..(segments.len() - 1) {
                abbrv.clear();
                let target_segment = String::from(&segments[i]);
                let abbreviated_segment = target_segment.chars().next().unwrap().to_string();

                if i > 0 {
                    abbrv.extend_from_slice(&segments[0..i])
                }
                abbrv.push(abbreviated_segment);
                abbrv.extend_from_slice(&segments[(i + 1)..]);

                self.insert(
                    abbrv.clone(),
                    String::from(search_term),
                    String::from(label),
                    MatchType::Abbreviated,
                );
                counter += 1;
            }
            pb.inc(1);
        }

        pb.finish_with_message(format!("Generated {} abbreviated entries", counter));
    }

    pub(crate) fn tokenize(&self, input: &str) -> (Vec<String>, Vec<(usize, usize)>) {
        self.tokenizer.tokenize(input)
    }

    pub(crate) fn tokenize_batch(&self, input: &[&str]) -> Vec<(Vec<String>, Vec<(usize, usize)>)> {
        self.tokenizer.encode_batch(input)
    }

    pub fn search<'a>(
        &self,
        text: &'a str,
        max_len: Option<usize>,
        result_selection: Option<&ResultSelection>,
    ) -> Vec<(String, Vec<Match>, usize, usize)> {
        let result_selection = result_selection.unwrap_or(&ResultSelection::LastPreferFull);
        let max_len = max_len.unwrap_or(self.tree_depth);

        let (mut slices, mut offsets) = self.tokenize(text);

        // Pad the slices and their offsets to include the last words
        slices.extend(vec![String::new(); max_len]);
        offsets.extend(vec![(0, 0); max_len]);
        let (slices, offsets) = (slices, offsets);

        let mut results = slices
            .par_windows(max_len)
            .map(|slice| self.traverse(slice.to_vec()))
            .zip(offsets.par_windows(max_len))
            .filter_map(|(result, offsets)| {
                if let Ok(result) = result {
                    Some((result, offsets))
                } else {
                    None
                }
            })
            .filter_map(|(result, offsets)| {
                if !result.is_empty() {
                    Some((result, offsets))
                } else {
                    None
                }
            })
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
                    ResultSelection::LastPreferFull => {
                        let result = results.last().unwrap();
                        let end = offsets[result.0.len() - 1].1;
                        if result
                            .1
                            .iter()
                            .any(|mtch| mtch.match_type == MatchType::Full)
                        {
                            let mut _matches = HashSet::new();
                            _matches.extend(result.1.iter().filter_map(|mtch| {
                                if mtch.match_type == MatchType::Full {
                                    Some(mtch.clone())
                                } else {
                                    None
                                }
                            }));
                            return vec![(result.0.join(" "), _matches, start, end)];
                        }
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

    pub(crate) fn traverse(
        &self,
        window: Vec<String>,
    ) -> Result<Vec<(Vec<String>, &HashSet<Match>)>, String> {
        let mut results = Vec::new();
        for i in 1..window.len() {
            let sub_window = window[0..=i].to_vec();
            if let Some(result) = self.search_map.get(&sub_window) {
                results.push((sub_window, result));
            }
        }
        if !results.is_empty() {
            Ok(results)
        } else {
            Err(String::from("No matches found"))
        }
    }
}

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use crate::tree::{HashMapSearchTree, ResultSelection};

    #[test]
    fn test_sample() {
        let mut tree = HashMapSearchTree::default();
        let entries: Vec<(String, String)> = vec![
            ("An example".to_string(), "uri:example".to_string()),
            ("An example phrase".to_string(), "uri:phrase".to_string()),
        ];
        tree.load(entries.clone(), false, 0, 0, false);
        let tree = tree;

        println!("{:?}", tree.search_map);

        let results = tree.search("An xyz", Some(3), None);
        assert!(results.is_empty());

        let results = tree.search(&entries[0].0, Some(3), Some(&ResultSelection::Last));
        println!("{:?}", results);
        let results = results.first().unwrap();
        let results = &results.1;
        assert_eq!(results.len(), 1);
        assert_eq!(&results[0].match_label, &entries[0].1);

        let results = tree.search(&entries[1].0, Some(3), Some(&ResultSelection::Last));
        println!("{:?}", results);
        let results = results.first().unwrap();
        let matches = &results.1;
        assert_eq!(matches.len(), 1);
        assert_eq!(&matches[0].match_label, &entries[1].1);

        let results = tree.search(&entries[1].0, Some(2), Some(&ResultSelection::Last));
        println!("{:?}", results);
        let results = results.first().unwrap();
        let matches = &results.1;
        assert_eq!(matches.len(), 1);
        assert_eq!(&matches[0].match_label, &entries[0].1);

        let results = tree.search(&entries[1].0, Some(3), Some(&ResultSelection::All));
        println!("{:?}", results);
        let matches: Vec<_> = results.into_iter().flat_map(|r| r.1).collect();
        assert_eq!(matches.len(), 2);
        let match_labels: Vec<String> = matches
            .into_iter()
            .map(|mtch| mtch.match_label.clone())
            .sorted()
            .collect();
        assert_eq!(match_labels, vec!["uri:example", "uri:phrase"]);
    }
}
