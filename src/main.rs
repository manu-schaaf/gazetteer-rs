#![feature(is_some_with)]

#[macro_use]
extern crate rocket;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env;

use itertools::Itertools;
#[cfg(feature = "gui")]
use rocket::form;
#[cfg(feature = "gui")]
use rocket::form::{Context, Contextual, Error, Form, FromForm};
#[cfg(feature = "gui")]
use rocket::fs::{FileServer, TempFile};
use rocket::fs::NamedFile;
#[cfg(feature = "gui")]
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::State;
#[cfg(feature = "gui")]
use rocket_dyn_templates::{context, Template};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use gazetteer::tree::{HashMapSearchTree, Match, ResultSelection};
use gazetteer::util::{CorpusFormat, read_lines};

#[cfg(test)]
mod rocket_test;

const DEFAULT_MAX_LEN: usize = 5;
const DEFAULT_GENERATE_ABBRV: bool = false;
const DEFAULT_GENERATE_SKIP_GRAMS: bool = false;
const DEFAULT_SKIP_GRAM_MAX_SKIPS: i32 = 2;
const DEFAULT_SKIP_GRAM_MIN_LENGTH: i32 = 2;

#[derive(Debug, Serialize, Deserialize)]
struct Request<'r> {
    text: Cow<'r, str>,
    max_len: Option<usize>,
    result_selection: Option<ResultSelection>,
}

#[get("/v1/communication_layer")]
async fn v1_communication_layer() -> Option<NamedFile> {
    NamedFile::open("communication_layer.lua").await.ok()
}

#[post("/v1/process", data = "<request>")]
async fn v1_process(
    request: Json<Request<'_>>,
    tree: &State<HashMapSearchTree>,
) -> Value {
    let results = tree.search(
        &request.text,
        request.max_len.or_else(|| Some(DEFAULT_MAX_LEN)),
        Option::from(&request.result_selection),
    );
    let results: Vec<Value> = results.into_iter()
        .map(|(string, mtches, begin, end)| {
            let match_labels = mtches.iter()
                .map(|mtch| &mtch.match_label)
                .join(" | ");
            let match_types = mtches.iter()
                .map(|mtch| mtch.match_type.to_string())
                .join(" | ");
            let match_strings = mtches.iter()
                .map(|mtch| &mtch.match_string)
                .join(" | ");
            json!({
                "string": string,
                "match_labels": match_labels,
                "match_types": match_types,
                "match_strings": match_strings,
                "begin": begin,
                "end": end,
            })
        }).collect::<Vec<Value>>();
    json!(results)
}

#[derive(Serialize, Deserialize)]
struct Config {
    filter_path: Option<String>,
    generate_abbrv: Option<bool>,
    generate_skip_grams: Option<bool>,
    skip_gram_min_length: Option<i32>,
    skip_gram_max_skips: Option<i32>,
    corpora: HashMap<String, Corpus>,
}

#[derive(Serialize, Deserialize)]
struct Corpus {
    path: String,
    filter_path: Option<String>,
    generate_abbrv: Option<bool>,
    generate_skip_grams: Option<bool>,
    skip_gram_min_length: Option<i32>,
    skip_gram_max_skips: Option<i32>,
    format: Option<CorpusFormat>,
}

fn parse_args_and_build_tree() -> HashMapSearchTree {
    let args: Vec<String> = env::args().collect();
    let config: String = if args.len() > 1 {
        std::fs::read_to_string(&args[1]).unwrap()
    } else {
        std::fs::read_to_string("config.toml").unwrap()
    };

    let config: Config = toml::from_str(&config).unwrap();

    let mut tree = HashMapSearchTree::default();
    let lines = config.filter_path.map_or_else(|| Vec::new(), |p| read_lines(&p));
    let filter_list = if lines.len() == 0 { None } else { Option::from(&lines) };

    for corpus in config.corpora.values() {
        let path: &String = &corpus.path;
        let generate_abbrv = corpus.generate_abbrv.unwrap_or_else(|| config.generate_abbrv.unwrap_or_else(|| DEFAULT_GENERATE_ABBRV));
        let generate_skip_grams = corpus.generate_skip_grams.unwrap_or_else(|| config.generate_skip_grams.unwrap_or_else(|| DEFAULT_GENERATE_SKIP_GRAMS));
        let skip_gram_min_length = corpus.skip_gram_min_length.unwrap_or_else(|| config.skip_gram_min_length.unwrap_or_else(|| DEFAULT_SKIP_GRAM_MIN_LENGTH));
        let skip_gram_max_skips = corpus.skip_gram_max_skips.unwrap_or_else(|| config.skip_gram_max_skips.unwrap_or_else(|| DEFAULT_SKIP_GRAM_MAX_SKIPS));
        let format = &corpus.format;
        if let Some(_filter_path) = &corpus.filter_path {
            let _lines: Vec<String> = read_lines(&_filter_path);
            let _filter_list = if _lines.len() == 0 { None } else { Option::from(&_lines) };
            tree.load_file(&path, generate_skip_grams, skip_gram_min_length, skip_gram_max_skips, _filter_list, generate_abbrv, format);
        } else {
            tree.load_file(&path, generate_skip_grams, skip_gram_min_length, skip_gram_max_skips, filter_list, generate_abbrv, format);
        }
    }
    println!("Finished loading gazetteer.");
    tree
}

#[cfg(not(feature = "gui"))]
#[launch]
fn rocket() -> _ {
    let tree: HashMapSearchTree = parse_args_and_build_tree();

    rocket::build()
        .mount("/", routes![v1_process, v1_communication_layer])
        .manage(tree)
}

#[cfg(feature = "gui")]
#[derive(Debug, FromForm)]
struct Submit<'v> {
    text: &'v str,
    file: TempFile<'v>,
    max_len: usize,
    result_selection: ResultSelection,
}

#[cfg(feature = "gui")]
fn file_or_text<'v>(text: &'v str, file: &TempFile) -> form::Result<'v, String> {
    if !(
        text.len() > 1 || file.content_type().is_some_and(|t| t.is_text())) {
        Err(Error::validation("You must either enter text or upload a file!"))?
    } else if !text.is_empty() {
        Ok(String::from(text))
    } else {
        Ok(read_lines(file.path().unwrap().to_str().unwrap()).join(""))
    }
}

#[cfg(feature = "gui")]
#[get("/")]
fn index() -> Template {
    Template::render("index", &Context::default())
}

#[cfg(feature = "gui")]
#[post("/", data = "<form>")]
fn submit<'r>(mut form: Form<Contextual<'r, Submit<'r>>>, tree: &State<HashMapSearchTree>) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            // println!("submission: {:#?}", submission);
            match file_or_text(submission.text, &submission.file) {
                Ok(text) => {
                    let results = tree.search(&text, Option::from(submission.max_len), Option::from(&submission.result_selection));
                    // for result in results.iter() {
                    //     println!("{:?} ({},{}) -> {:?}", result.0, result.2, result.2, result.1)
                    // }
                    Template::render("success", context! {
                        text: text,
                        results: results,
                    })
                }
                Err(errs) => {
                    for err in errs {
                        form.context.push_error(err.with_name("file"));
                    }
                    Template::render("index", &form.context)
                }
            }
        }
        None => Template::render("index", &form.context),
    };

    (form.context.status(), template)
}

#[cfg(feature = "gui")]
#[post("/search", format = "json", data = "<request>")]
async fn search(
    request: Json<Request<'_>>,
    tree: &State<HashMapSearchTree>,
) -> Value {
    let results = tree.search(
        &request.text,
        request.max_len.or_else(|| Some(DEFAULT_MAX_LEN)),
        Option::from(&request.result_selection),
    );
    json!({
        "status": "ok",
        "results": results
    })
}

#[cfg(feature = "gui")]
#[catch(500)]
fn search_error() -> Value {
    json!({
        "status": "error",
        "reason": "An error occurred during tree search."
    })
}

#[cfg(feature = "gui")]
#[launch]
fn rocket() -> _ {
    let tree: HashMapSearchTree = parse_args_and_build_tree();

    rocket::build()
        .mount("/", routes![index, submit, search, v1_process, v1_communication_layer])
        .register("/search", catchers![search_error])
        .attach(Template::fairing())
        .mount("/", FileServer::from("static"))
        .manage(tree)
}