use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::io::{BufRead, Read};
use std::path::Path;
use std::str::FromStr;

use csv::{Reader, ReaderBuilder, Trim};
use flate2::bufread::GzDecoder;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tokenizers::{Normalizer, NormalizerWrapper, OffsetReferential, OffsetType, PreTokenizedString, PreTokenizer, PreTokenizerWrapper, SplitDelimiterBehavior};
use tokenizers::normalizers::{Lowercase, NFKC};
use tokenizers::normalizers::Sequence as NormalizerSequence;
use tokenizers::pre_tokenizers::punctuation::Punctuation;
use tokenizers::pre_tokenizers::sequence::Sequence as PreTokenizerSequence;
use tokenizers::pre_tokenizers::whitespace::Whitespace;

use crate::tree::MatchType;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct CorpusFormat {
    /// The comment character. Defaults to b'#'.
    pub comment: Option<String>,
    /// The delimiter character. Defaults to b'\t'.
    pub delimiter: Option<String>,
    /// If true, quotes are escaped as double quotes instead of backslash escaped.
    /// Defaults to false.
    pub double_quote: Option<bool>,
    /// If true, the number of columns can vary. Defaults to false.
    pub flexible: Option<bool>,
    /// If true, the first line of the input will be treated as a header. Defaults to false.
    pub has_header: Option<bool>,
    /// The quoting character. Defaults to b'"'
    pub quote: Option<String>,
    /// If disabled, quotes will not be treated differently. Enabled by default.
    pub quoting: Option<bool>,
    /// If given, skips the first n-lines of a document, i.e. to skip metadata.
    pub skip_lines: Option<usize>,
    /// The column index of the search term in the input table. Defaults to 0.
    pub search_term_column_idx: Option<usize>,
    /// The column index of the label in the input table. Defaults to 1.
    pub label_column_idx: Option<usize>,
    /// If given, will insert the label into the format string by replacing the pattern string with
    /// the label.
    pub label_format_string: Option<String>,
    /// The label pattern string, i.e. the part of the label_format_string that is replaced with
    /// the label. Defaults to '{}'.
    pub label_format_pattern: Option<String>,
}

pub fn read_lines(filename: &str) -> Vec<String> {
    let extension = match Path::new(filename.clone()).extension() {
        None => { "" }
        Some(ext) => {
            ext.to_str().unwrap()
        }
    };
    let file = File::open(Path::new(filename)).expect("Could not open file");
    let reader = io::BufReader::new(file);
    match extension {
        "gz" => {
            let mut s = String::new();
            GzDecoder::new(reader).read_to_string(&mut s);
            s.lines().map(|s| String::from(s)).collect::<Vec<String>>()
        }
        _ => {
            reader.lines().filter_map(|line| line.ok()).collect::<Vec<String>>()
        }
    }
}

pub fn read_csv(filename: &str, format: &CorpusFormat) -> Vec<(String, String)> {
    let extension = match Path::new(filename.clone()).extension() {
        None => { "" }
        Some(ext) => {
            ext.to_str().unwrap()
        }
    };
    let file = File::open(Path::new(filename)).expect("Could not open file");

    let mut buf_reader = io::BufReader::new(file);
    if let Some(skip) = format.skip_lines {
        let mut temp = String::new();
        for i in 0..skip {
            buf_reader.read_line(&mut temp).expect(
                &format!("Reached EOF after skipping {} lines!", i)
            );
        }
    }
    let mut buf_reader: Box<dyn Read> = match extension {
        "gz" => {
            Box::new(GzDecoder::new(buf_reader))
        }
        _ => {
            Box::new(buf_reader)
        }
    };

    let search_term_column_idx = format.search_term_column_idx.unwrap_or(0);
    let label_column_idx = format.label_column_idx.unwrap_or(1);
    let label_format_pattern = format.label_format_pattern.clone().unwrap_or(String::from("{}"));

    ReaderBuilder::new()
        .comment(format.comment.clone().map_or(Some(b'#'), |s| s.bytes().next()))
        .delimiter(format.delimiter.clone().map_or(b'\t', |s| s.bytes().next().unwrap()))
        .double_quote(format.double_quote.unwrap_or(false))
        .flexible(format.flexible.unwrap_or(false))
        .has_headers(format.has_header.unwrap_or(false))
        .quote(format.quote.clone().map_or(b'"', |s| s.bytes().next().unwrap()))
        .quoting(format.quoting.unwrap_or(false))
        .trim(Trim::All)
        .from_reader(buf_reader).records()
        .filter_map(|row| row.ok())
        .filter(|row| !row.is_empty())
        .map(|row| match &format.label_format_string {
            None => (String::from(&row[search_term_column_idx]), String::from(&row[label_column_idx])),
            Some(format_string) => (
                String::from(&row[search_term_column_idx]),
                format_string.replace(&label_format_pattern, &row[label_column_idx])
            )
        })
        .collect::<Vec<(String, String)>>()
}

pub fn get_files(root_path: &str) -> Vec<String> {
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

pub fn split_with_indices(s: String) -> (Vec<String>, Vec<(usize, usize)>) {
    let indices = s.match_indices(SPLIT_PATTERN).collect::<Vec<_>>();

    let mut last = 0;
    let mut offsets: Vec<(usize, usize)> = Vec::new();
    let mut slices: Vec<String> = Vec::new();
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

fn _push_slice(slices: &mut Vec<String>, offsets: &mut Vec<(usize, usize)>, slice: &str, last: usize, idx: usize) {
    if slice.len() > 1 || slice.len() == 1 && !SPLIT_PATTERN.contains(&slice.chars().next().unwrap()) {
        offsets.push((last.clone(), idx.clone() + 1));
        slices.push(String::from(slice));
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

pub fn parse_files<>(files: Vec<String>, pb: Option<&ProgressBar>, format: &Option<CorpusFormat>, filter_list: Option<&Vec<String>>) -> Vec<(String, String)> {
    let format: CorpusFormat = match format {
        None => { CorpusFormat::default() }
        Some(format) => { format.clone() }
    };

    let filter_list: HashSet<String> = filter_list
        .map_or_else(
            || HashSet::new(),
            |list| list.iter()
                .map(|s| s.to_lowercase())
                .collect::<HashSet<String>>(),
        );
    files.par_iter()
        .flat_map_iter(|file| {
            let pairs = read_csv(file, &format);
            if let Some(pb) = pb {
                pb.inc(1);
            }
            pairs
        })
        .filter(|(search_term, _)| {
            filter_list.len() == 0 || !filter_list.contains(&search_term.to_lowercase())
        })
        .collect::<Vec<(String, String)>>()
}

#[derive(Debug)]
pub struct Tokenizer {
    normalizer: NormalizerWrapper,
    pre_tokenizer: PreTokenizerWrapper,
}

impl Tokenizer {
    pub fn default() -> Tokenizer {
        Tokenizer {
            normalizer: NormalizerWrapper::Sequence(NormalizerSequence::new(vec![
                NormalizerWrapper::Lowercase(Lowercase),
                NormalizerWrapper::NFKC(NFKC::default()),
            ])),
            pre_tokenizer: PreTokenizerWrapper::Sequence(PreTokenizerSequence::new(vec![
                PreTokenizerWrapper::Punctuation(Punctuation::new(SplitDelimiterBehavior::Removed)),
                PreTokenizerWrapper::Whitespace(Whitespace::default()),
            ])),
        }
    }

    pub fn tokenize(&self, string: &str) -> (Vec<String>, Vec<(usize, usize)>) {
        let mut string = PreTokenizedString::from(string);
        string.normalize(|s| self.normalizer.normalize(s)).expect("Failed during normalization!");
        self.pre_tokenizer.pre_tokenize(&mut string).expect("Failed during pre-tokenization!");
        let mut tokens = Vec::new();
        let mut offsets = Vec::new();
        for (slice, offset, _) in string.get_splits(OffsetReferential::Original, OffsetType::Char) {
            tokens.push(String::from(slice));
            offsets.push((offset.0, offset.1));
        }
        (tokens, offsets)
    }

    pub fn encode_batch(
        &self,
        inputs: &[&str],
    ) -> Vec<(Vec<String>, Vec<(usize, usize)>)> {
        let pb = ProgressBar::new(inputs.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Tokenizing Inputs {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let encodings = inputs
            .into_par_iter()
            .map(|input| {
                let tokenized = self.tokenize(input);
                pb.inc(1);
                tokenized
            })
            .collect::<Vec<(Vec<String>, Vec<(usize, usize)>)>>();
        pb.finish_with_message("Done");
        encodings
    }
}


pub fn create_skip_grams(mut items: Vec<Vec<String>>, max_skips: i32, min_length: i32) -> Vec<Vec<String>> {
    if max_skips > 0 && items.iter().all(|item| item.len() > min_length as usize) {
        let mut deleted = Vec::new();
        for item in items {
            let item = item.clone();
            let mut d: Vec<String> = Vec::new();
            let l = item.len();
            for i in 1..l {
                d.clear();
                d.extend_from_slice(&item[..i]);
                d.extend_from_slice(&item[i + 1..]);
                deleted.push(d.clone());
            }
        }
        deleted.append(&mut create_skip_grams(deleted.clone(), max_skips - 1, min_length));
        return deleted;
    } else {
        return items;
    }
}

pub fn parse_optional<I: FromStr>(string: &Option<String>) -> Option<I> {
    string.as_ref().map_or(None, |s| s.parse::<I>().map_or(None, |val| Some(val)))
}
