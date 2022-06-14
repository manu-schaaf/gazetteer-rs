use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::collections::vec_deque::VecDeque;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use ngrams::Ngram;
use rayon::prelude::*;
#[cfg(feature = "server")]
use rocket::FromFormField;
use rocket::http::ext::IntoCollection;
#[cfg(feature = "server")]
use rocket::serde::{Deserialize, Serialize};

use crate::util::{get_files, get_spinner, parse_files, split_with_indices};

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

    fn load(&mut self, root_path: &str, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Input Files {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let mut lines = parse_files(files, Option::from(&pb), filter_list);

        if generate_ngrams {
            let ngrams = Self::generate_ngrams(&lines);

            let pb = ProgressBar::new(lines.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Loading n-grams {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            self.load_lines(ngrams, Some(&pb));
            pb.finish_with_message("Done");
        }

        if generate_abbrv {
            let abbreviations = Self::generate_abbreviations(&lines);

            let pb = ProgressBar::new(lines.len() as u64);
            pb.set_style(ProgressStyle::with_template(
                "Loading abbreviations {bar:40} {pos}/{len} {msg}"
            ).unwrap());
            self.load_lines(abbreviations, Some(&pb));
            pb.finish_with_message("Done");
        }

        let pb = ProgressBar::new(lines.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading lines {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        self.load_lines(lines.into_iter().map(|line| (line.0, line.1, MatchType::Full)).collect(), Some(&pb));
        pb.finish_with_message("Done");
    }

    fn generate_ngrams(lines: &Vec<(String, String, MatchType)>) -> Vec<(String, String, MatchType)> {
        let filtered = lines.par_iter()
            .filter(|(taxon_name, _, _)| split_with_indices(&taxon_name).0.len() > 2)
            .collect::<Vec<&(String, String, MatchType)>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Generating N-Grams {bar:40} {pos}/{len} {msg}"
        ).unwrap());

        let mut ngrams = filtered.par_iter()
            .map(|(taxon_name, uri, _)| {
                let mut result = Vec::new();
                let ngrams = split_with_indices(&taxon_name).0.into_iter()
                    .ngrams(2)
                    .pad()
                    .collect::<Vec<Vec<&str>>>();
                for ngram in ngrams {
                    // Check whether any part is an abbreviation
                    if ngram.iter().all(|el| el.len() > 2) {
                        result.push((ngram.join(" "), String::from(uri), MatchType::NGram));
                    }
                }
                pb.inc(1);
                result
            })
            .flatten()
            .collect::<Vec<(String, String, MatchType)>>();

        pb.finish_with_message(format!("Adding {} n-grams", ngrams.len()));
        ngrams
    }

    fn generate_abbreviations(lines: &Vec<(String, String, MatchType)>) -> Vec<(String, String, MatchType)> {
        let filtered = lines.par_iter()
            .filter(|(taxon_name, _, _)| taxon_name.split(" ").collect::<Vec<_>>().len() > 1)
            .collect::<Vec<&(String, String, MatchType)>>();

        let pb = ProgressBar::new(filtered.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Generating Abbreviations {bar:40} {pos}/{len} {msg}"
        ).unwrap());

        let mut abbrevations = filtered.par_iter()
            .map(|(taxon_name, uri, _)| {
                let mut result = Vec::new();

                let string = taxon_name.clone();
                let clone = string.split(" ").collect::<Vec<_>>();
                let head = String::from(clone[0]);
                let first_char = head.chars().next().unwrap().to_string();
                let mut abbrv = vec![first_char.as_str()];
                abbrv.extend_from_slice(&clone[1..]);
                result.push((abbrv.join(" "), String::from(uri), MatchType::Abbreviated));

                pb.inc(1);
                result
            })
            .flatten()
            .collect::<Vec<(String, String, MatchType)>>();

        pb.finish_with_message("Done");
        abbrevations
    }

    fn load_lines(&mut self, lines: Vec<(String, String, MatchType)>, pb: Option<&ProgressBar>);

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &HashSet<Match>)>, String> {
        let vec = self.traverse_internal(values, Vec::new(), Vec::new());
        if vec.len() > 0 {
            Ok(vec)
        } else {
            Err(String::from("No matches found"))
        }
    }

    fn traverse_internal<'a>(
        &'a self,
        values: VecDeque<&'a str>,
        matched_string_buffer: Vec<&'a str>,
        results: Vec<(Vec<&'a str>, &'a HashSet<Match>)>,
    ) -> Vec<(Vec<&'a str>, &'a HashSet<Match>)>;

    fn search<'a>(&self, text: &'a str, max_len: Option<usize>, result_selection: Option<&ResultSelection>) -> Vec<(String, HashSet<Match>, usize, usize)> {
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

#[derive(Debug, Clone)]
pub struct HashMapSearchTree {
    pub matches: HashSet<Match>,
    pub children: HashMap<String, HashMapSearchTree>,
}

impl SearchTree for HashMapSearchTree {
    fn default() -> Self where Self: Sized {
        HashMapSearchTree {
            matches: HashSet::new(),
            children: HashMap::new(),
        }
    }

    fn load_lines(&mut self, lines: Vec<(String, String, MatchType)>, pb: Option<&ProgressBar>) {
        for (taxon, uri, match_type) in lines {
            self.insert(VecDeque::from(split_with_indices(&taxon.clone()).0), taxon, uri, match_type);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn traverse_internal<'a>(
        &'a self,
        mut values: VecDeque<&'a str>,
        mut matched_string_buffer: Vec<&'a str>,
        mut results: Vec<(Vec<&'a str>, &'a HashSet<Match>)>,
    ) -> Vec<(Vec<&'a str>, &'a HashSet<Match>)> {
        let value = values.pop_front().expect("");
        match self.children.get(&value.to_lowercase()) {
            Some(child) => {
                matched_string_buffer.push(value);
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
        }
    }

    fn child() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, mut values: VecDeque<&str>, match_string: String, match_uri: String, match_type: MatchType) {
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

#[deprecated = "Superseded by HashMapSearchTree"]
#[derive(Debug, Clone)]
pub struct BinarySearchTree {
    pub value: String,
    pub matches: HashSet<Match>,
    pub children: Vec<BinarySearchTree>,
}

impl SearchTree for BinarySearchTree {
    fn default() -> Self {
        Self {
            value: "<ROOT>".to_string(),
            matches: HashSet::new(),
            children: vec![],
        }
    }

    fn load(&mut self, root_path: &str, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) {
        let files: Vec<String> = get_files(root_path);
        println!("Found {} files", files.len());

        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading Input Files {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let mut lines = parse_files(files, Option::from(&pb), filter_list);

        let mut additional: Vec<(String, String, MatchType)> = Vec::new();
        if generate_ngrams {
            additional.append(&mut Self::generate_ngrams(&lines));
        }

        if generate_abbrv {
            additional.append(&mut Self::generate_abbreviations(&lines));
        }
        lines.append(&mut additional);

        let lines = Self::sort_and_dedup_lines(lines);

        let pb = ProgressBar::new(lines.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Loading lines {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        self.load_lines(lines, Some(&pb));
        pb.finish_with_message("Done");
    }

    fn load_lines(&mut self, lines: Vec<(String, String, MatchType)>, pb: Option<&ProgressBar>) {
        for (taxon_name, uri, match_type) in lines {
            self.insert(VecDeque::from(split_with_indices(&taxon_name.clone()).0), taxon_name, uri, match_type);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }
    }

    fn traverse_internal<'a>(
        &'a self,
        mut values: VecDeque<&'a str>,
        mut matched_string_buffer: Vec<&'a str>,
        mut results: Vec<(Vec<&'a str>, &'a HashSet<Match>)>,
    ) -> Vec<(Vec<&'a str>, &'a HashSet<Match>)> {
        let value = values.pop_front().expect("");
        match self.children.binary_search_by_key(&value.to_lowercase().as_str(), |a| a.get_value()) {
            Ok(idx) => {
                matched_string_buffer.push(value);
                if !self.children[idx].matches.is_empty() {
                    results.push((matched_string_buffer.clone(), &self.children[idx].matches));
                }

                if !values.is_empty() {
                    self.children[idx].traverse_internal(values, matched_string_buffer, results)
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

impl BinarySearchTree {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            value: "<ROOT>".to_string(),
            matches: HashSet::new(),
            children: Vec::with_capacity(capacity),
        }
    }

    fn with_capacity_from_sorted(capacity: usize, lines: Vec<(String, String, MatchType)>, pb: Option<&ProgressBar>) -> Self {
        let mut tree = Self {
            value: "<ROOT>".to_string(),
            matches: HashSet::new(),
            children: Vec::with_capacity(capacity),
        };

        for (taxon_name, uri, match_type) in lines {
            tree.insert_in_order(VecDeque::from(split_with_indices(&taxon_name.clone()).0), taxon_name, uri, match_type);

            if let Some(pb) = pb {
                pb.inc(1)
            }
        }

        tree
    }

    fn from(value: &str, match_string: String, match_uri: String) -> Self {
        let value = String::from(value);
        Self {
            value,
            matches: HashSet::from([Match::full(match_string, match_uri)]),
            children: vec![],
        }
    }

    fn child(value: &str) -> Self {
        let value = String::from(value);
        Self {
            value,
            matches: HashSet::new(),
            children: vec![],
        }
    }

    fn sort_and_dedup_lines(mut lines: Vec<(String, String, MatchType)>) -> Vec<(String, String, MatchType)> {
        let pb = get_spinner();
        pb.set_message(format!("Sorting {} lines..", lines.len()));
        lines.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        // lines.sort_by_key(|(taxon, uri, mtch)| (taxon, uri));
        pb.finish();

        let pb = get_spinner();
        pb.set_message(format!("Dropping duplicates.."));
        lines.dedup_by(|a, b| b.0.eq(&a.0) && b.1.eq(&a.1));
        pb.finish();

        lines
    }

    fn get_value(&self) -> &String {
        &self.value
    }

    pub fn insert(&mut self, mut values: VecDeque<&str>, match_string: String, match_uri: String, match_type: MatchType) {
        let value = &values.pop_front().unwrap().to_lowercase();
        match self.children.binary_search_by_key(&value, |a| a.get_value()) {
            Ok(idx) => {
                if values.is_empty() {
                    self.children[idx].matches.insert(Match { match_type, match_string, match_uri });
                } else {
                    self.children[idx].insert(values, match_string, match_uri, match_type);
                }
            }
            Err(idx) => {
                if values.is_empty() {
                    self.children.insert(idx, BinarySearchTree::from(value, match_string, match_uri));
                } else {
                    self.children.insert(idx, BinarySearchTree::child(value));
                    self.children[idx].insert(values, match_string, match_uri, match_type);
                }
            }
        }
    }

    pub fn insert_in_order(&mut self, mut values: VecDeque<&str>, match_string: String, match_uri: String, match_type: MatchType) {
        let value = &values.pop_front().unwrap().to_lowercase();
        if let Some(mut last_child) = self.children.last_mut() && last_child.value.eq(value) {
            if values.is_empty() {
                last_child.matches.insert(Match { match_type, match_string, match_uri });
            } else {
                last_child.insert_in_order(values, match_string, match_uri, match_type);
            }
        } else {
            if values.is_empty() {
                self.children.push(BinarySearchTree::from(value, match_string, match_uri));
            } else {
                self.children.push(BinarySearchTree::child(value));
                self.children.last_mut().unwrap().insert_in_order(values, match_string, match_uri, match_type);
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
}

#[deprecated = "Superseded by HashMapSearchTree"]
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

    fn load_lines(&mut self, lines: Vec<(String, String, MatchType)>, pb: Option<&ProgressBar>) {
        let mut start_end: Vec<(usize, usize)> = Vec::new();
        for start in (0..lines.len()).step_by(self.each_size) {
            let size = usize::min(start + self.each_size, lines.len());
            start_end.push((start, size));
        }

        let mp = MultiProgress::new();
        let mut tasks: Vec<(&[(String, String, MatchType)], ProgressBar)> = Vec::new();
        for (start, end) in start_end {
            let pb = mp.add(ProgressBar::new((end - start) as u64));
            pb.set_style(ProgressStyle::with_template(&format!(
                "Building Split {:>2}/{} {{bar:40}} {{pos}}/{{len}} {{msg}}",
                end / self.each_size,
                lines.len() / self.each_size
            )).unwrap());
            tasks.push((&lines[start..end], pb));
        }

        let mut results = tasks.par_iter()
            .map(|(lines, pb)| {
                let tree = BinarySearchTree::with_capacity_from_sorted(self.each_size, Vec::from(*lines), Option::from(pb));
                pb.finish();
                tree
            }).collect::<Vec<BinarySearchTree>>();
        self.children.append(&mut results);
    }

    fn traverse<'a>(&'a self, values: VecDeque<&'a str>) -> Result<Vec<(Vec<&'a str>, &HashSet<Match>)>, String> {
        let results = self.children.par_iter()
            .filter_map(|tree| tree.traverse(values.clone()).ok())
            .flatten()
            .collect::<Vec<(Vec<&str>, &HashSet<Match>)>>();
        if results.is_empty() {
            Err(String::from("No matches found"))
        } else {
            Ok(results)
        }
    }

    fn traverse_internal<'a>(&'a self, values: VecDeque<&'a str>, matched_string_buffer: Vec<&'a str>, results: Vec<(Vec<&'a str>, &'a HashSet<Match>)>) -> Vec<(Vec<&'a str>, &'a HashSet<Match>)> {
        unimplemented!()
    }
}

impl MultiTree {
    fn with_each_size(root_path: &str, each_size: i32, generate_ngrams: bool, generate_abbrv: bool, filter_list: Option<&Vec<String>>) -> Self {
        let mut root = Self {
            children: vec![],
            each_size: each_size as usize,
        };

        root.load(root_path, generate_ngrams, generate_abbrv, filter_list);

        root
    }
}