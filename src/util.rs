use std::collections::HashSet;
use std::fs::File;
use std::io;
use std::io::{BufRead, Read};
use std::path::Path;
use std::str::FromStr;

use anyhow::anyhow;
use anyhow::Context;
use csv::{ReaderBuilder, Trim};
use flate2::bufread::GzDecoder;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use tokenizers::normalizers::Sequence as NormalizerSequence;
use tokenizers::normalizers::{Lowercase, NFKC};
use tokenizers::pre_tokenizers::punctuation::Punctuation;
use tokenizers::pre_tokenizers::sequence::Sequence as PreTokenizerSequence;
use tokenizers::pre_tokenizers::whitespace::Whitespace;
use tokenizers::{
    Normalizer, NormalizerWrapper, OffsetReferential, OffsetType, PreTokenizedString, PreTokenizer,
    PreTokenizerWrapper, SplitDelimiterBehavior,
};

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

pub struct RobustCorpusFormat {
    /// The comment character. Defaults to b'#'.
    pub comment: Option<u8>,
    /// The delimiter character. Defaults to b'\t'.
    pub delimiter: u8,
    /// If true, quotes are escaped as double quotes instead of backslash escaped.
    /// Defaults to false.
    pub double_quote: bool,
    /// If true, the number of columns can vary. Defaults to false.
    pub flexible: bool,
    /// If true, the first line of the input will be treated as a header. Defaults to false.
    pub has_header: bool,
    /// The quoting character. Defaults to b'"'
    pub quote: u8,
    /// If disabled, quotes will not be treated differently. Enabled by default.
    pub quoting: bool,
    /// If given, skips the first n-lines of a document, i.e. to skip metadata.
    pub skip_lines: usize,
    /// The column index of the search term in the input table. Defaults to 0.
    pub search_term_column_idx: usize,
    /// The column index of the label in the input table. Defaults to 1.
    pub label_column_idx: usize,
    /// If given, will insert the label into the format string by replacing the pattern string with
    /// the label.
    pub label_format_string: Option<String>,
    /// The label pattern string, i.e. the part of the label_format_string that is replaced with
    /// the label. Defaults to '{}'.
    pub label_format_pattern: String,
}

impl Default for RobustCorpusFormat {
    fn default() -> Self {
        RobustCorpusFormat {
            comment: Some(b'#'),
            delimiter: b'\t',
            double_quote: false,
            flexible: false,
            has_header: false,
            quote: b'"',
            quoting: true,
            skip_lines: 0,
            search_term_column_idx: 0,
            label_column_idx: 1,
            label_format_string: None,
            label_format_pattern: String::from("{}"),
        }
    }
}

impl TryFrom<CorpusFormat> for RobustCorpusFormat {
    type Error = anyhow::Error;

    fn try_from(format: CorpusFormat) -> Result<Self, Self::Error> {
        let default = RobustCorpusFormat::default();
        let robust_corpus_format = RobustCorpusFormat {
            comment: format.comment.map_or(default.comment, |s| s.bytes().next()),
            delimiter: format
                .delimiter
                .map_or(Some(b'\t'), |s| s.bytes().next())
                .context("Could not get delimiter character")?,
            double_quote: format.double_quote.unwrap_or(default.double_quote),
            flexible: format.flexible.unwrap_or(default.flexible),
            has_header: format.has_header.unwrap_or(default.has_header),
            quote: format
                .quote
                .map_or(Some(b'"'), |s| s.bytes().next())
                .context("Could not get quote character")?,
            quoting: format.quoting.unwrap_or(false),
            skip_lines: format.skip_lines.unwrap_or(default.skip_lines),
            search_term_column_idx: format
                .search_term_column_idx
                .unwrap_or(default.search_term_column_idx),
            label_column_idx: format.label_column_idx.unwrap_or(default.label_column_idx),
            label_format_string: format.label_format_string,
            label_format_pattern: format
                .label_format_pattern
                .unwrap_or(default.label_format_pattern),
        };
        if let Some(label_format_string) = &robust_corpus_format.label_format_string {
            if !label_format_string.contains(&robust_corpus_format.label_format_pattern) {
                return Err(anyhow!(
                    "The label format string must contain the label format pattern"
                ));
            }
        }
        Ok(robust_corpus_format)
    }
}

pub fn read_lines(filename: &str) -> Vec<String> {
    let extension = match Path::new(filename).extension() {
        None => "",
        Some(ext) => ext.to_str().unwrap(),
    };
    let file = File::open(Path::new(filename)).expect("Could not open file");
    let reader = io::BufReader::new(file);
    match extension {
        "gz" => {
            let mut s = String::new();
            GzDecoder::new(reader)
                .read_to_string(&mut s)
                .expect("Failed to decode file with .gz extension.");
            s.lines().map(String::from).collect::<Vec<String>>()
        }
        _ => reader
            .lines()
            .filter_map(|line| line.ok())
            .collect::<Vec<String>>(),
    }
}

pub fn read_csv(filename: &str, format: &CorpusFormat) -> anyhow::Result<Vec<(String, String)>> {
    let extension = match Path::new(filename).extension() {
        None => "",
        Some(ext) => ext.to_str().unwrap(),
    };
    let file = File::open(Path::new(filename)).context("Could not open file")?;

    let mut buf_reader = io::BufReader::new(file);

    let format =
        RobustCorpusFormat::try_from(format.clone()).context("Failed to convert corpus format")?;

    if format.skip_lines > 0 {
        let mut temp = String::new();
        for i in 0..format.skip_lines {
            buf_reader
                .read_line(&mut temp)
                .context(format!("Reached EOF after skipping {i} lines"))?;
        }
    }
    let buf_reader: Box<dyn Read> = match extension {
        "gz" => Box::new(GzDecoder::new(buf_reader)),
        _ => Box::new(buf_reader),
    };

    let search_term_column_idx = format.search_term_column_idx;
    let label_column_idx = format.label_column_idx;
    let label_format_pattern = format.label_format_pattern;

    let reader = ReaderBuilder::new()
        .comment(format.comment)
        .delimiter(format.delimiter)
        .double_quote(format.double_quote)
        .flexible(format.flexible)
        .has_headers(format.has_header)
        .quote(format.quote)
        .quoting(format.quoting)
        .trim(Trim::All)
        .from_reader(buf_reader)
        .records()
        .filter_map(std::result::Result::ok)
        .filter(|row| !row.is_empty())
        .map(|row| {
            format.label_format_string.as_ref().map_or_else(
                || {
                    (
                        String::from(&row[search_term_column_idx]),
                        String::from(&row[label_column_idx]),
                    )
                },
                |format_string| {
                    (
                        String::from(&row[search_term_column_idx]),
                        format_string.replace(&label_format_pattern, &row[label_column_idx]),
                    )
                },
            )
        })
        .collect::<Vec<(String, String)>>();
    Ok(reader)
}

#[must_use]
pub fn get_files(root_path: &str) -> Vec<String> {
    let mut files = glob(root_path)
        .expect("Failed to read glob pattern")
        .filter_map(|file| file.ok())
        .filter(|file| file.metadata().unwrap().is_file())
        .map(|file| String::from(file.as_path().to_str().unwrap()))
        .collect::<Vec<String>>();
    files.sort_by_key(|a| a.to_lowercase());
    files
}

pub const SPLIT_PATTERN: &[char; 10] = &[' ', '.', ',', ':', ';', '-', '_', '"', '(', ')'];

#[must_use]
pub fn split_with_indices(s: &str) -> (Vec<String>, Vec<(usize, usize)>) {
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

fn _push_slice(
    slices: &mut Vec<String>,
    offsets: &mut Vec<(usize, usize)>,
    slice: &str,
    last: usize,
    idx: usize,
) {
    if slice.len() > 1
        || slice.len() == 1 && !SPLIT_PATTERN.contains(&slice.chars().next().unwrap())
    {
        offsets.push((last, idx + 1));
        slices.push(String::from(slice));
    }
}

pub fn parse_files(
    files: &Vec<String>,
    pb: Option<&ProgressBar>,
    format: &Option<CorpusFormat>,
    filter_list: &Option<Vec<String>>,
) -> anyhow::Result<Vec<(String, String)>> {
    let format: CorpusFormat = match format {
        None => CorpusFormat::default(),
        Some(format) => format.clone(),
    };

    let filter_list: HashSet<String> = filter_list.clone().map_or_else(HashSet::new, |list| {
        list.iter()
            .map(|s| s.to_lowercase())
            .collect::<HashSet<String>>()
    });
    let parsed_files: Result<Vec<Vec<(String, String)>>, anyhow::Error> = files
        .par_iter()
        .map(|file| {
            let pairs = read_csv(file, &format)?;
            if let Some(pb) = pb {
                pb.inc(1);
            }
            Ok(pairs)
        })
        .collect();
    Ok(parsed_files?
        .into_iter()
        .flatten()
        .filter(|(search_term, _)| {
            filter_list.is_empty() || !filter_list.contains(&search_term.to_lowercase())
        })
        .collect::<Vec<(String, String)>>())
}

#[derive(Debug)]
pub struct Tokenizer {
    normalizer: NormalizerWrapper,
    pre_tokenizer: PreTokenizerWrapper,
}

impl Tokenizer {
    pub fn tokenize(&self, string: &str) -> (Vec<String>, Vec<(usize, usize)>) {
        let mut string = PreTokenizedString::from(string);
        string
            .normalize(|s| self.normalizer.normalize(s))
            .expect("Failed during normalization!");
        self.pre_tokenizer
            .pre_tokenize(&mut string)
            .expect("Failed during pre-tokenization!");
        let mut tokens = Vec::new();
        let mut offsets = Vec::new();
        for (slice, offset, _) in string.get_splits(OffsetReferential::Original, OffsetType::Char) {
            tokens.push(String::from(slice));
            offsets.push((offset.0, offset.1));
        }
        (tokens, offsets)
    }

    pub fn encode_batch(&self, inputs: &[&str]) -> Vec<(Vec<String>, Vec<(usize, usize)>)> {
        let pb = ProgressBar::new(inputs.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("Tokenizing Inputs {bar:40} {pos}/{len} {msg}").unwrap(),
        );
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

impl Default for Tokenizer {
    fn default() -> Tokenizer {
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
}

pub fn create_skip_grams(
    items: Vec<Vec<String>>,
    max_skips: i32,
    min_length: i32,
) -> Vec<Vec<String>> {
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
        deleted.append(&mut create_skip_grams(
            deleted.clone(),
            max_skips - 1,
            min_length,
        ));
        deleted
    } else {
        items
    }
}

pub fn parse_optional<I: FromStr>(string: &Option<String>) -> Option<I> {
    string
        .as_ref()
        .and_then(|s| s.parse::<I>().map_or(None, |val| Some(val)))
}
