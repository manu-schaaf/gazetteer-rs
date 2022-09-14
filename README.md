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
