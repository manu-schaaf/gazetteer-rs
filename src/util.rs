use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io;
use std::io::{BufRead, Read};
use std::path::Path;

use flate2::bufread::GzDecoder;
use glob::glob;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use tokenizers::{DecoderWrapper, Encoding, Model, ModelWrapper, Normalizer, NormalizerWrapper, OffsetReferential, OffsetType, PostProcessorWrapper, PreTokenizedString, PreTokenizer, PreTokenizerWrapper, SplitDelimiterBehavior, TokenizerBuilder, TokenizerImpl};
use tokenizers::models::wordlevel::WordLevel;
use tokenizers::normalizers::{Lowercase, NFKC};
use tokenizers::normalizers::Sequence as NSequence;
use tokenizers::parallelism::MaybeParallelIterator;
use tokenizers::pre_tokenizers::punctuation::Punctuation;
use tokenizers::pre_tokenizers::sequence::Sequence as PTSequence;
use tokenizers::pre_tokenizers::whitespace::Whitespace;
use tokenizers::Tokenizer as TTokenizer;

use crate::tree::MatchType;

pub fn read_lines(filename: &str) -> Vec<String> {
    let extension = Path::new(filename.clone()).extension().unwrap().to_str().unwrap();
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
            let uri = if split.len() > 1 {
                String::from(split[1])
            } else {
                String::with_capacity(0)
            };
            (taxon, uri, MatchType::Full)
        })
        .filter(|(taxon, _, _)| {
            filter_list.len() == 0 || !filter_list.contains(&taxon.to_lowercase())
        })
        .collect::<Vec<(String, String, MatchType)>>()
}

#[derive(Debug)]
pub struct Tokenizer {
    tokenizer: TTokenizer,
}

impl Tokenizer {
    pub fn default() -> Tokenizer {
        let vocab: HashMap<String, u32> = HashMap::from([("[UNK]".to_string(), 0)]);
        let model: WordLevel = WordLevel::builder().vocab(vocab).unk_token("[UNK]".to_string()).build().expect("Failed to build model!");
        let normalizer_wrapper = NormalizerWrapper::Sequence(NSequence::new(vec![
            NormalizerWrapper::Lowercase(Lowercase),
            NormalizerWrapper::NFKC(NFKC::default()),
        ]));
        let pre_tokenizer_wrapper = PreTokenizerWrapper::Sequence(PTSequence::new(vec![
            PreTokenizerWrapper::Punctuation(Punctuation::new(SplitDelimiterBehavior::Removed)),
            PreTokenizerWrapper::Whitespace(Whitespace::default()),
        ]));
        let tokenizer_impl: TokenizerImpl<ModelWrapper, NormalizerWrapper, PreTokenizerWrapper, PostProcessorWrapper, DecoderWrapper> = TokenizerBuilder::new()
            .with_model(ModelWrapper::WordLevel(model))
            .with_normalizer(Some(normalizer_wrapper))
            .with_pre_tokenizer(Some(pre_tokenizer_wrapper))
            .with_post_processor(None)
            .with_decoder(None).build().expect("Failed to build tokenizer!");
        let tokenizer: TTokenizer = TTokenizer::from(
            tokenizer_impl
        );
        Tokenizer {
            tokenizer
        }
    }

    pub fn from_file(path: &str) -> Tokenizer {
        let tokenizer = tokenizers::Tokenizer::from_file(path)
            .expect(format!("Failed to load tokenizer from path: '{}'", path).as_str());
        Tokenizer {
            tokenizer
        }
    }

    pub fn extend(&mut self, new_entries: &Vec<String>) {
        let normalizer = self.tokenizer.get_normalizer();
        let pre_tokenizer = self.tokenizer.get_pre_tokenizer().expect("Missing pre-tokenizer!");

        let split: fn(&str, Option<&NormalizerWrapper>, &PreTokenizerWrapper) -> Vec<String> = if normalizer.is_some() {
            |string: &str, normalizer, pre_tokenizer| -> Vec<String> {
                let mut pts = PreTokenizedString::from(string);
                pts.normalize(|s| normalizer.unwrap().normalize(s));
                pre_tokenizer.pre_tokenize(&mut pts);
                pts.get_splits(OffsetReferential::Original, OffsetType::Char)
                    .into_iter()
                    .map(|(slice, _, _)| String::from(slice))
                    .collect::<Vec<String>>()
            }
        } else {
            |string: &str, normalizer, pre_tokenizer| -> Vec<String> {
                let mut pts = PreTokenizedString::from(string);
                pre_tokenizer.pre_tokenize(&mut pts);
                pts.get_splits(OffsetReferential::Original, OffsetType::Char)
                    .into_iter()
                    .map(|(slice, _, _)| String::from(slice))
                    .collect::<Vec<String>>()
            }
        };

        let pb = ProgressBar::new(new_entries.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Extending Tokenizer {bar:40} {pos}/{len} {msg}"
        ).unwrap());
        let words = new_entries.par_iter()
            .map(|s| {
                let segments = split(s, normalizer, pre_tokenizer);
                pb.inc(1);
                segments
            })
            .flatten()
            .collect::<Vec<String>>();
        pb.finish_with_message("Done");

        let old_vocab = self.tokenizer.get_model().get_vocab();
        let old_vocab_size = old_vocab.len();
        let old_vocab_keys: HashSet<String> = HashSet::from_iter(old_vocab.keys().map(|s| String::from(s)));
        let new_vocab: HashSet<String> = HashSet::from_iter(HashSet::from_iter(words.into_iter()).difference(&old_vocab_keys).map(|s| String::from(s)));

        println!("Adding {} new tokens", new_vocab.len());

        let joined_vocab = HashMap::from_iter(
            old_vocab.into_iter()
                .chain(new_vocab.into_iter()
                    .enumerate()
                    .map(|(i, w)| (w, (i + old_vocab_size) as u32))
                )
        );

        let new_model = WordLevel::builder().vocab(joined_vocab).unk_token("[UNK]".to_string()).build().expect("Failed to build model!");
        self.tokenizer.with_model(ModelWrapper::WordLevel(new_model));
    }

    pub fn decode(&self, tokens: &Vec<u32>) -> Vec<String> {
        tokens.iter().map(|t| self.tokenizer.id_to_token(*t).unwrap()).collect::<Vec<String>>()
    }

    pub fn encode_batch(
        &self,
        inputs: Vec<String>,
    ) -> Vec<(Vec<u32>, Vec<(usize, usize)>)> {
        let pb = ProgressBar::new(inputs.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "Tokenizing Inputs {bar:40} {pos}/{len} {msg}"
        ).unwrap());

        let encodings = inputs
            .into_par_iter()
            .map(|input| {
                let e = self.tokenizer.encode_char_offsets(input, false);
                pb.inc(1);
                e
            })
            .filter_map(|result| result.ok())
            .map(|encoding| (Vec::from(encoding.get_ids()), Vec::from(encoding.get_offsets())))
            .collect::<Vec<(Vec<u32>, Vec<(usize, usize)>)>>();

        pb.finish_with_message("Done");

        encodings
    }
}
