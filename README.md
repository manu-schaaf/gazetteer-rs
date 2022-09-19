# gazetteer-rs

Rust implemenatation of a skip-gram gazetteer tagger

[![version](https://img.shields.io/github/license/texttechnologylab/gazetteer-rs)]()
[![build](https://github.com/texttechnologylab/gazetteer-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/texttechnologylab/gazetteer-rs/actions)
[![latest](https://img.shields.io/github/v/release/texttechnologylab/gazetteer-rs)]()

## Description

This project can be used to find large numbers of search terms in documents in linear time with respect to the document length.

### Process

Generally, the process is as follows.
1. Given an input tab-separated list of search-term-label-pairs, will construct a search tree of words.
2. Then, for each document, iterate over the words in the document and traverse the tree if a matching word is found.

## Details

To create the tree, the input lists are segmented using a pre-tokenizer from the [`tokenizers`](https://docs.rs/tokenizers/) library.

The tree is backed by Rusts standard library [`HashMap`](https://doc.rust-lang.org/std/collections/struct.HashMap.html).

The RESTful interface is implemented using [`rocket`](https://docs.rs/rocket/).

There are configuration options to enable the abbrevation of search terms or the creation of n-grams from search term segments, see [`config.toml`](/config.toml).

### Tree Properties

It is important to note, that multiple occurrences of search terms in the input data will result in multiple labels on the leafs of the search tree.
This is especially true if n-grams are generated.
Each resulting match is returned alongside its match type, which may either be `MatchType::Full`, `MatchType::Abbreviated` or `MatchType::NGram`.

See below for an example tree given the input:

```tsv
Puffinus          https://www.gbif.org/species/5229335
Puffinus puffinus https://www.gbif.org/species/5229380
Sula              https://www.gbif.org/species/2480966
Sula bassana      https://www.gbif.org/species/4352320
Sula leucogaster  https://www.gbif.org/species/2480975
```

![Example Tree](https://user-images.githubusercontent.com/34143268/190984324-38e7380b-5bdf-48c6-9ed1-59e1c3b25a34.png)
Here, the orange nodes represent nodes within the search tree and purple nodes represent `Full` type matches with their corresponding labels.

See below for a small tree given the following list, with n-grams enabled:
```tsv
Sula                          https://www.gbif.org/species/2480966
Sula bassana                  https://www.gbif.org/species/4352320
Sula leucogaster              https://www.gbif.org/species/2480975
Sula leucogaster leucogaster  https://www.gbif.org/species/7192777
```

<img src="https://user-images.githubusercontent.com/34143268/190987006-bfd401e4-1122-40a9-965a-6cc18d022809.png" width="400">

Here, the light green node represents a `NGram` type match.

### TextImager 2.0 Interface

Supports the new TextImager interface `v1`. See:

- [main.rs&rightarrow;v1_communication_layer()](src/main.rs#L45) and [main.rs&rightarrow;v1_process()](src/main.rs#L50) for API methods.
- [communication_layer.lua](communication_layer.lua) for the communication layer.
- See [TTLab-UIMA](https://github.com/texttechnologylab/TTLab-UIMA) for DUUI bindings and more variants.

###  GUI

You can also build the tool with `--features gui` to enable an additional user interface that allows tagging small texts or uploading small plaintext files for tagging.
This requires an additional dependency: [rocket_dyn_templates](https://docs.rs/rocket_dyn_templates)

#### Screenshots
##### Screenshot of the GUI

![The GUI.](https://user-images.githubusercontent.com/34143268/188922452-c26962e1-f1cf-4d68-8536-690386681f6a.png)

##### Screenshot of the Results

![The output of the GUI is human-readable.](https://user-images.githubusercontent.com/34143268/188922364-015553bd-33ad-428b-8634-8dc6ff4af4a3.png)

## Note

- This project is currently still a work-in-progress.
- See [biofid-gazetteer](https://github.com/texttechnologylab/biofid-gazetteer) for a Java implementation of a similar tool that integrates with the [TextImager](https://github.com/texttechnologylab/textimager-uima) pipeline.
