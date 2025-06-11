use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::hash::Hash;
use std::sync::Arc;

use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::util::{
    create_skip_grams, get_files, parse_files, CorpusFormat, Tokenizer, TokensAndOffsets,
};

#[derive(Debug, Serialize, Deserialize)] // FIXME
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
    const fn get_value(&self) -> i32 {
        match self {
            Self::None => -1,
            Self::Full => 0,
            Self::Abbreviated => 1,
            Self::SkipGram => 2,
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
            Self::None => {
                write!(f, "None")
            }
            Self::Full => {
                write!(f, "Full")
            }
            Self::Abbreviated => {
                write!(f, "Abbreviated")
            }
            Self::SkipGram => {
                write!(f, "SkipGram")
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Match {
    pub match_type: MatchType,
    pub match_string: Arc<String>,
    pub match_label: Arc<String>,
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
        write!(
            f,
            "{} Match: {} -> {}",
            self.match_type, self.match_string, self.match_label
        )
    }
}

#[derive(Debug, Default)]
pub struct HashMapSearchTree {
    pub search_map: HashMap<Vec<String>, HashSet<Match>>,
    tokenizer: Tokenizer,
    tree_depth: usize,
}

type EntryType = (Vec<String>, Arc<String>, Arc<String>);

impl HashMapSearchTree {
    #[allow(clippy::too_many_arguments)]
    pub fn load_file(
        &mut self,
        root_path: &str,
        generate_skip_grams: bool,
        skip_gram_min_length: i32,
        skip_gram_max_skips: i32,
        filter_list: &Option<Vec<String>>,
        generate_abbrv: bool,
        abbrv_max_index: i32,
        abbrv_min_suffix_length: i32,
        format: &Option<CorpusFormat>,
    ) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files to read", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Loading Input Files {bar:40} {pos}/{len} {msg}").unwrap(),
        );
        let lines: Vec<(String, String)> =
            parse_files(&files, Option::from(&pb), format, filter_list)
                .expect("Failed to parse an input file");
        pb.finish_with_message("Done");

        self.load(
            lines,
            generate_skip_grams,
            skip_gram_min_length,
            skip_gram_max_skips,
            generate_abbrv,
            abbrv_max_index,
            abbrv_min_suffix_length,
        );
    }

    pub fn load(
        &mut self,
        entries: Vec<(String, String)>,
        generate_skip_grams: bool,
        skip_gram_min_length: i32,
        skip_gram_max_skips: i32,
        generate_abbrv: bool,
        abbrv_max_index: i32,
        abbrv_min_suffix_length: i32,
    ) {
        let search_terms: Vec<&str> = entries.iter().map(|line| line.0.as_str()).collect();
        let segmented: Vec<TokensAndOffsets> = self.tokenize_batch(search_terms.as_slice());
        let entries: Vec<EntryType> = segmented
            .into_iter()
            .zip(entries)
            .map(|(segments, (search_term, label))| {
                (segments.0, Arc::from(search_term), Arc::from(label))
            })
            .collect();

        self.load_entries(&entries);

        if generate_skip_grams {
            self.generate_skip_grams(&entries, skip_gram_min_length, skip_gram_max_skips);
        }

        if generate_abbrv {
            self.generate_abbreviations(&entries, abbrv_max_index, abbrv_min_suffix_length);
        }
    }

    pub(crate) fn load_entries(&mut self, entries: &Vec<EntryType>) {
        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Loading Entries {bar:40} {pos}/{len} {msg}").unwrap(),
        );

        for (segments, search_term, label) in entries {
            self.insert(
                segments.clone(),
                search_term.clone(),
                label.clone(),
                MatchType::Full,
            );
            pb.inc(1);
        }
        pb.finish_with_message("Done");
    }

    pub fn insert(
        &mut self,
        segments: Vec<String>,
        match_string: Arc<String>,
        match_label: Arc<String>,
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
        lines: &[EntryType],
        min_length: i32,
        max_skips: i32,
    ) {
        let filtered = lines
            .iter()
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
                    skip_gram,
                    search_term.clone(),
                    label.clone(),
                    MatchType::SkipGram,
                );
                counter += 1;
            }
            pb.inc(1);
        }
        pb.finish_with_message(format!("Generated {counter} skip-grams"));
    }

    pub(crate) fn generate_abbreviations(
        &mut self,
        lines: &[EntryType],
        abbrv_max_index: i32,
        abbrv_min_suffix_length: i32,
    ) {
        let filtered = lines
            .iter()
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
            // set the maximum abbreviated segment index to the last segment
            // UNLESS the user has specified a maximum index (abbrv_max_index > 0)
            let max_index = segments.len() - 1;
            let max_index = if abbrv_max_index < 0 || abbrv_max_index > (max_index as i32) {
                max_index
            } else {
                abbrv_max_index as usize
            };
            for i in 0..=max_index {
                // check if the remaining segments after the abbreviated segment are long enough
                // i.e., "Thing A" -> "T A"
                let suffix_length: usize = segments[(i + 1)..].iter().map(String::len).sum();
                if abbrv_min_suffix_length > 0 && suffix_length < (abbrv_min_suffix_length as usize)
                {
                    continue;
                }

                abbrv.clear();
                let target_segment = String::from(&segments[i]);
                let abbreviated_segment = target_segment.chars().next().unwrap().to_string();

                // if we are not abbreviating the first segment, add the prefix segments in any case
                if i > 0 {
                    abbrv.extend_from_slice(&segments[0..i]);
                }
                abbrv.push(abbreviated_segment);
                abbrv.extend_from_slice(&segments[(i + 1)..]);

                self.insert(
                    abbrv.clone(),
                    search_term.clone(),
                    label.clone(),
                    MatchType::Abbreviated,
                );
                counter += 1;
            }
            pb.inc(1);
        }

        pb.finish_with_message(format!("Generated {} abbreviated entries", counter));
    }

    pub(crate) fn tokenize(&self, input: &str) -> TokensAndOffsets {
        self.tokenizer.tokenize(input)
    }

    pub(crate) fn tokenize_batch(&self, input: &[&str]) -> Vec<TokensAndOffsets> {
        self.tokenizer.encode_batch(input)
    }

    pub fn search<'a>(
        &'a self,
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
            .map(|slice| self.traverse(slice))
            .zip(offsets.par_windows(max_len))
            .filter_map(|(result, offsets)| result.map_or(None, |result| Some((result, offsets))))
            .filter_map(|(result, offsets)| {
                if result.is_empty() {
                    None
                } else {
                    Some((result, offsets))
                }
            })
            .map(|(results, offsets)| {
                let start = offsets[0].0;
                match result_selection {
                    ResultSelection::All => {
                        let mut returns = Vec::new();
                        for result in results {
                            let end = offsets[result.search_terms.len() - 1].1;
                            returns.push((
                                result.get_search_term_string(),
                                result.get_search_results(),
                                start,
                                end,
                            ));
                        }
                        returns
                    }
                    ResultSelection::Last => {
                        let result = results.last().unwrap();
                        let end = offsets[result.search_terms.len() - 1].1;
                        vec![(
                            result.get_search_term_string(),
                            result.get_search_results(),
                            start,
                            end,
                        )]
                    }
                    ResultSelection::LastPreferFull => {
                        let result = results.last().unwrap();
                        let end = offsets[result.search_terms.len() - 1].1;
                        if result
                            .search_results
                            .iter()
                            .any(|mtch| mtch.match_type == MatchType::Full)
                        {
                            let mut mtches = Vec::new();
                            mtches.extend(result.get_search_results().into_iter().filter_map(
                                |mtch| {
                                    if mtch.match_type == MatchType::Full {
                                        Some(mtch)
                                    } else {
                                        None
                                    }
                                },
                            ));
                            return vec![(result.get_search_term_string(), mtches, start, end)];
                        }
                        vec![(
                            result.get_search_term_string(),
                            result.get_search_results(),
                            start,
                            end,
                        )]
                    }
                }
            })
            .flatten()
            // .map(|(s, mtches, a, b)| (s, mtches.into_iter().sorted().collect::<Vec<&Match>>(), a, b))
            .collect::<Vec<(String, Vec<Match>, usize, usize)>>();

        // results.dedup_by(|b, a| b.2 <= a.3);
        // TODO: This removes fully covered entities that end on the same character as their covering entities but not partial overlaps
        results.dedup_by_key(|el| el.3);

        results
    }

    pub(crate) fn traverse(&self, window: &[String]) -> Result<Vec<TraversalResult>, String> {
        let mut results = Vec::new();
        for i in 0..window.len() {
            let search_terms = window[0..=i].to_vec();
            if let Some(search_results) = self.search_map.get(&search_terms) {
                results.push(TraversalResult {
                    search_terms,
                    search_results,
                });
            }
        }
        if results.is_empty() {
            Err(String::from("No matches found"))
        } else {
            Ok(results)
        }
    }
}

pub struct TraversalResult<'a> {
    search_terms: Vec<String>,
    search_results: &'a HashSet<Match>,
}

impl TraversalResult<'_> {
    fn get_search_term_string(&self) -> String {
        self.search_terms.join(" ")
    }
    fn get_search_results(&self) -> Vec<Match> {
        self.search_results.iter().cloned().sorted().collect()
    }
}

#[cfg(test)]
mod test {
    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_sample() {
        let mut tree = HashMapSearchTree::default();
        let an_example: String = "An example".to_string();
        let an_example_phrase: String = "An example phrase".to_string();
        let example: String = "Example".to_string();
        let entries: Vec<(String, String)> = vec![
            (an_example.clone(), "uri:example".to_string()),
            (an_example_phrase.clone(), "uri:phrase".to_string()),
            (example.clone(), "uri:single".to_string()),
        ];
        tree.load(entries.clone(), false, 0, 0, false, 0, 3);
        let tree = tree;

        println!("{:?}", tree.search_map);

        let results = tree.search("An xyz", Some(3), None);
        assert!(results.is_empty());

        let results = tree.search(&an_example, Some(3), Some(&ResultSelection::Last));
        println!("{results:?}");
        let results = results.first().unwrap();
        let results = &results.1;
        assert_eq!(results.len(), 1);
        assert_eq!(&*results[0].match_label, &entries[0].1);

        let results = tree.search(&an_example_phrase, Some(3), Some(&ResultSelection::Last));
        println!("{results:?}");
        let results = results.first().unwrap();
        let matches = &results.1;
        assert_eq!(matches.len(), 1);
        assert_eq!(&*matches[0].match_label, &entries[1].1);

        let results = tree.search(&example, Some(3), None);
        println!("{results:?}");
        let results = results.first().unwrap();
        let results = &results.1;
        assert_eq!(results.len(), 1);
        assert_eq!(&*results[0].match_label, &entries[2].1);

        let results = tree.search(&an_example_phrase, Some(2), Some(&ResultSelection::Last));
        println!("{results:?}");
        let results = results.first().unwrap();
        let matches = &results.1;
        assert_eq!(matches.len(), 1);
        assert_eq!(&*matches[0].match_label, &entries[0].1);

        let results = tree.search(&an_example_phrase, Some(3), Some(&ResultSelection::All));
        println!("{results:?}");
        let matches: Vec<_> = results.into_iter().flat_map(|r| r.1).collect();
        assert_eq!(matches.len(), 3);
        let match_labels: Vec<String> = matches
            .into_iter()
            .map(|mtch| (*mtch.match_label).clone())
            .sorted()
            .collect();
        assert_eq!(
            match_labels,
            vec!["uri:example", "uri:phrase", "uri:single"]
        );
    }

    #[test]
    fn test_skip_grams() {
        let mut tree = HashMapSearchTree::default();
        let entries: Vec<(String, String)> = vec![
            ("An example".to_string(), "uri:example".to_string()),
            ("An example phrase".to_string(), "uri:phrase".to_string()),
            ("Another example A".to_string(), "uri:other".to_string()),
        ];
        tree.load(entries.clone(), true, 2, 2, false, 0, 3);
        let tree = tree;

        println!("{:?}", tree.search_map);

        let results = tree.search("An xyz", Some(3), None);
        assert!(results.is_empty());

        let results = tree.search("An A A xyz ", Some(3), None);
        assert!(results.is_empty());

        let results: Vec<(String, Vec<crate::tree::Match>, usize, usize)> =
            tree.search(&entries[0].0, Some(3), Some(&ResultSelection::Last));
        println!("{results:?}");
        let results = results.first().unwrap();
        let results = &results.1;
        assert_eq!(results.len(), 2);
        assert_eq!(&*results[0].match_label, &entries[0].1);

        let results = tree.search(&entries[1].0, Some(3), Some(&ResultSelection::Last));
        println!("{results:?}");
        let results = results.first().unwrap();
        let matches = &results.1;
        assert_eq!(matches.len(), 1);
        assert_eq!(&*matches[0].match_label, &entries[1].1);

        let results = tree.search(&entries[1].0, Some(2), Some(&ResultSelection::Last));
        println!("{results:?}");
        let results = results.first().unwrap();
        let matches = &results.1;
        assert_eq!(matches.len(), 2);
        assert_eq!(&*matches[0].match_label, &entries[0].1);

        let results = tree.search(&entries[1].0, Some(3), Some(&ResultSelection::All));
        println!("{results:?}");
        let matches: Vec<_> = results.into_iter().flat_map(|r| r.1).collect();
        assert_eq!(matches.len(), 3);
        let match_labels: Vec<String> = matches
            .into_iter()
            .map(|mtch| (*mtch.match_label).clone())
            .sorted()
            .collect();
        assert_eq!(
            match_labels,
            vec!["uri:example", "uri:phrase", "uri:phrase"]
        );
    }
}
