use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::io::BufRead;
use std::path::Path;
use std::slice::IterMut;
use std::vec::IntoIter;

use encode_unicode::{SliceExt, StrExt};
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use rayon::prelude::*;

use crate::tree::MatchType;

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

pub const SPLIT_PATTERN: &[char; 11] = &[' ', '.', ',', ':', ';', '-', '_', '"', '(', ')', '×'];

pub fn split_with_indices(s: &str) -> (Vec<&str>, Vec<(usize, usize)>) {
    let indices = s.match_indices(SPLIT_PATTERN).collect::<Vec<_>>();

    let mut last = 0;
    let mut offsets: Vec<(usize, usize)> = Vec::new();
    let mut slices: Vec<&str> = Vec::new();
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

pub fn parse_files<>(files: Vec<String>, pb: Option<&ProgressBar>, filter_list: Option<&Vec<String>>) -> Vec<(String, String, MatchType)> {
    let filter_list: HashSet<String> = filter_list
        .map_or_else(
            || HashSet::new(),
            |list| list.iter()
                .map(|s| s.to_lowercase())
                .collect::<HashSet<String>>(),
        );
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
            (taxon, uri, MatchType::Full)
        })
        .filter(|(taxon, _, _)| {
            filter_list.len() == 0 || !filter_list.contains(&taxon.to_lowercase())
        })
        .collect::<Vec<(String, String, MatchType)>>()
}

pub fn get_deletes(string: &String, max_deletes: usize) -> Vec<String> {
    let indices: Vec<usize> = string.as_bytes().utf8char_indices().into_iter().map(|(offset, _, _)| offset).collect();
    let deletes: Vec<String> = indices.par_windows(2)
        .map(|idx| delete(string, 0, max_deletes, idx[0], idx[1]))
        .flatten()
        .collect();
    deletes
}

fn delete(last: &String, current_deletes: usize, max_deletes: usize, split_index: usize, next_index: usize) -> Vec<String> {
    let mut deletes = Vec::new();
    let _current_deletes = current_deletes + 1;
    let (a, b) = last.split_at(split_index);
    let deleted = format!("{}{}", a, b[next_index - split_index..].to_string());
    deletes.push(deleted.clone());
    if _current_deletes < max_deletes {
        let indices: Vec<usize> = deleted.as_bytes().utf8char_indices().into_iter().map(|(offset, _, _)| offset).collect();
        for idx in indices.windows(2) {
            deletes.append(&mut delete(&deleted, _current_deletes, max_deletes, idx[0], idx[1]))
        }
    }
    deletes
}