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

use crate::StringTree;

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

pub fn split_with_indices(s: &str) -> (Vec<(usize, usize)>, Vec<&str>) {
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