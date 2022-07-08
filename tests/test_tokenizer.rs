use std::collections::{HashMap, HashSet};

use rayon::prelude::{*};
use tokenizers::{DecoderWrapper, ModelWrapper, Normalizer, NormalizerWrapper, PostProcessorWrapper, PreTokenizedString, PreTokenizer, PreTokenizerWrapper, TokenizerBuilder, TokenizerImpl, Trainer};
use tokenizers::models::wordlevel::{WordLevel, WordLevelBuilder, WordLevelTrainer};
use tokenizers::normalizers::{Lowercase, NFKC};
use tokenizers::normalizers::Sequence as NSequence;
use tokenizers::pre_tokenizers::punctuation::Punctuation;
use tokenizers::pre_tokenizers::sequence::Sequence as PTSequence;
use tokenizers::pre_tokenizers::whitespace::Whitespace;

use gazetteer::util::{parse_files, Tokenizer};

#[test]
fn test_from_file() {
    let builder: TokenizerBuilder<ModelWrapper, NormalizerWrapper, PreTokenizerWrapper, PostProcessorWrapper, DecoderWrapper> = TokenizerBuilder::new()
        .with_model(ModelWrapper::WordLevel(WordLevelBuilder::new().vocab(HashMap::from([("[UNK]".to_string(), 0)])).unk_token("[UNK]".to_string()).build().unwrap()))
        .with_normalizer(Some(NormalizerWrapper::NFKC(NFKC::default())))
        .with_pre_tokenizer(Some(PreTokenizerWrapper::Sequence(PTSequence::new(vec![
            PreTokenizerWrapper::Punctuation(Punctuation::default()),
            PreTokenizerWrapper::Whitespace(Whitespace::default()),
        ]))))
        .with_post_processor(None)
        .with_decoder(None);

    let tokenizer = builder.build().unwrap();
    tokenizer.save("resources/tokenizer.json", false).expect("Failed to save tokenizer!");
    println!("{:?}", tokenizer.encode("An example-sentence.", false).unwrap().get_tokens());

    let tokenizer = Tokenizer::from_file("resources/tokenizer.json");
    println!("{:?}", tokenizer.tokenize("An example-sentence."));
}