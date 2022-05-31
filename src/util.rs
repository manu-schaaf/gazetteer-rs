use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io;
use std::io::{BufRead, Lines};
use std::path::Path;

use glob::glob;
use indicatif::{ProgressBar, ProgressIterator, ProgressStyle};
use itertools::{EitherOrBoth, merge_join_by};
use ngrams::Ngrams;
use rayon::prelude::*;
use symspell::{DistanceAlgorithm, SymSpell, SymSpellBuilder, UnicodeiStringStrategy};

pub fn read_lines<P>(filename: P) -> Vec<String>
    where P: AsRef<Path>, {
    let file = File::open(filename).expect("Could not open file");
    let lines = io::BufReader::new(file).lines();
    lines.filter_map(|line| line.ok()).collect::<Vec<String>>()
}

pub fn get_files(root_path: &str) -> Vec<String> {
    println!("Reading resources dir...");
    let mut files = glob(root_path).expect("Failed to read glob pattern")
        .into_iter()
        .filter_map(|file| file.ok())
        .filter(|file| file.metadata().unwrap().is_file())
        .map(|file| String::from(file.as_path().to_str().unwrap()))
        .collect::<Vec<String>>();
    files.sort_by_key(|a| a.to_lowercase());
    files
}

pub const SPLIT_PATTERN: &[char; 10] = &[' ', '.', ',', ':', ';', '-', '_', '"', '(', ')'];

pub fn split_with_indices(s: &str) -> (Vec<&str>, Vec<(usize, usize)>) {
    let indices = s.match_indices(SPLIT_PATTERN).collect::<Vec<_>>();

    let mut last = 0;
    let mut offsets: Vec<((usize, usize))> = Vec::new();
    let mut slices: Vec<(&str)> = Vec::new();
    for (idx, mtch) in indices {
        let slice = &s[last..idx];
        _push_slice(&mut slices, &mut offsets, slice, last, idx);
        last = idx + mtch.len();
    }
    if last < s.len() {
        _push_slice(&mut slices, &mut offsets, &s[last..s.len()], last, s.len());
    }

    (slices, offsets)
}

fn _push_slice<'a>(slices: &mut Vec<&'a str>, offsets: &mut Vec<(usize, usize)>, slice: &'a str, last: usize, idx: usize) {
    if slice.len() > 1 || slice.len() == 1 && !SPLIT_PATTERN.contains(&slice.chars().next().unwrap()) {
        offsets.push((last.clone(), idx.clone() + 1));
        slices.push(slice);
    }
}

pub(crate) fn get_spinner() -> ProgressBar {
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
    pb
}

pub fn parse_files<>(files: Vec<String>, pb: Option<&ProgressBar>, filter_list: Option<&Vec<String>>) -> Vec<(String, String)> {
    files.par_iter()
        .map(|file| {
            let lines = read_lines(file);
            if let Some(pb) = pb {
                pb.inc(1);
            }
            lines
        })
        .flatten()
        .map(|line| line.trim().to_string())
        .filter(|line| line.len() > 0)
        .map(|line| {
            let split = line.split('\t').collect::<Vec<&str>>();
            let taxon = String::from(split[0]);
            let uri = String::from(split[1]);
            (taxon, uri)
        })
        .filter(|(taxon, _)| {
            if let Some(filter_list) = filter_list {
                filter_list.binary_search(&taxon.to_lowercase()).is_err()
            } else {
                true
            }
        })
        .collect::<Vec<(String, String)>>()
}


struct Segmenter {
    normalizer: NormalizerWrapper,
    pre_tokenizer: Sequence,
}

impl Segmenter {
    fn default() -> Segmenter {
        Self {
            normalizer: NormalizerWrapper::NFKC(NFKC::default()),
            pre_tokenizer: Sequence::new(vec![
                PreTokenizerWrapper::Punctuation(Punctuation::new(SplitDelimiterBehavior::Removed)),
                PreTokenizerWrapper::Whitespace(Whitespace::default()),
            ]),
        }
    }

    pub fn tokenize(&self, string: &str) -> Vec<(String, (usize, usize))> {
        let mut string = PreTokenizedString::from(string);
        string.normalize(|s| self.normalizer.normalize(s)).expect("Failed during normalization!");
        self.pre_tokenizer.pre_tokenize(&mut string).expect("Failed during pre-tokenization!");
        string.get_splits(OffsetReferential::Original, OffsetType::Char)
            .into_iter()
            .map(
                |(token, offset, _)| (String::from(token), (offset.0, offset.1))
            ).collect()
    }
}