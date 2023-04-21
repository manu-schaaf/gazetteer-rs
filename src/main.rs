use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;

use clap::{arg, command, Command};
use itertools::Itertools;

// #[cfg(feature = "gui")]
// use rocket::form;
// #[cfg(feature = "gui")]
// use rocket::form::{Context, Contextual, Error, Form, FromForm};
// #[cfg(feature = "gui")]
// use rocket::fs::{FileServer, TempFile};
// #[cfg(feature = "gui")]
// use rocket::http::Status;
// #[cfg(feature = "gui")]
// use rocket_dyn_templates::{context, Template};

use actix_files::NamedFile;
use actix_web::{web, App, HttpResponse, HttpServer, Result};

use anyhow::Context;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use gazetteer::tree::{HashMapSearchTree, ResultSelection};
use gazetteer::util::{parse_optional, read_lines, CorpusFormat};

const DEFAULT_GENERATE_ABBRV: bool = false;
const DEFAULT_GENERATE_SKIP_GRAMS: bool = false;
const DEFAULT_SKIP_GRAM_MAX_SKIPS: i32 = 2;
const DEFAULT_SKIP_GRAM_MIN_LENGTH: i32 = 2;

#[cfg(debug)]
const LOG_LEVEL: &str = "debug";
#[cfg(not(debug))]
const LOG_LEVEL: &str = "warn";

struct AppState {
    tree: HashMapSearchTree,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProcessRequest<'r> {
    text: Cow<'r, str>,
    max_len: Option<String>,
    result_selection: Option<ResultSelection>,
}

async fn v1_communication_layer() -> Result<NamedFile> {
    Ok(NamedFile::open_async("communication_layer.lua").await?)
}

async fn v1_process(
    request: web::Json<ProcessRequest<'_>>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let results = state.get_ref().tree.search(
        &request.text,
        parse_optional::<usize>(&request.max_len),
        Option::from(&request.result_selection),
    );
    let results: Vec<Value> = results
        .into_iter()
        .map(|(string, mtches, begin, end)| {
            let match_labels = mtches.iter().map(|mtch| &mtch.match_label).join(" | ");
            let match_types = mtches
                .iter()
                .map(|mtch| mtch.match_type.to_string())
                .join(" | ");
            let match_strings = mtches.iter().map(|mtch| &mtch.match_string).join(" | ");
            json!({
                "string": string,
                "match_labels": match_labels,
                "match_types": match_types,
                "match_strings": match_strings,
                "begin": begin,
                "end": end,
            })
        })
        .collect::<Vec<Value>>();
    HttpResponse::Ok().json(results)
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

fn cli() -> Command {
    command!().args([
        arg!(config: [CONFIG] "Path to the config file to use. Defaults to 'config.toml'.")
            .default_value("config.toml"),
    ])
}

fn parse_args_and_build_tree() -> anyhow::Result<HashMapSearchTree> {
    let args = cli().get_matches();
    let config_path: &String = args.get_one("config").context("Error in arguments!")?;
    let config: String =
        std::fs::read_to_string(config_path).context("Failed to load configuration.")?;

    let config: Config = toml::from_str(&config).context("Failed to parse configuration TOML")?;

    let mut tree = HashMapSearchTree::default();
    let default_filter_list = load_filter_list(config.filter_path);

    for corpus in config.corpora.values() {
        let path: &String = &corpus.path;
        let generate_abbrv = corpus
            .generate_abbrv
            .unwrap_or_else(|| config.generate_abbrv.unwrap_or(DEFAULT_GENERATE_ABBRV));
        let generate_skip_grams = corpus.generate_skip_grams.unwrap_or_else(|| {
            config
                .generate_skip_grams
                .unwrap_or(DEFAULT_GENERATE_SKIP_GRAMS)
        });
        let skip_gram_min_length = corpus.skip_gram_min_length.unwrap_or_else(|| {
            config
                .skip_gram_min_length
                .unwrap_or(DEFAULT_SKIP_GRAM_MIN_LENGTH)
        });
        let skip_gram_max_skips = corpus.skip_gram_max_skips.unwrap_or_else(|| {
            config
                .skip_gram_max_skips
                .unwrap_or(DEFAULT_SKIP_GRAM_MAX_SKIPS)
        });
        let format = &corpus.format;
        if let Some(filter_path) = &corpus.filter_path {
            let lines: Vec<String> = read_lines(filter_path);
            let filter_list = if lines.is_empty() {
                None
            } else {
                Option::from(lines)
            };
            tree.load_file(
                path,
                generate_skip_grams,
                skip_gram_min_length,
                skip_gram_max_skips,
                &filter_list,
                generate_abbrv,
                format,
            );
        } else {
            tree.load_file(
                path,
                generate_skip_grams,
                skip_gram_min_length,
                skip_gram_max_skips,
                &default_filter_list,
                generate_abbrv,
                format,
            );
        }
    }
    println!("Finished loading gazetteer.");
    Ok(tree)
}

fn load_filter_list(filter_path: Option<String>) -> Option<Vec<String>> {
    let lines = filter_path.map_or_else(Vec::new, |p| read_lines(&p));
    if lines.is_empty() {
        None
    } else {
        Option::from(lines)
    }
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or(LOG_LEVEL));

    let state: Arc<AppState> = Arc::new(AppState {
        tree: parse_args_and_build_tree()?,
    });
    let data: web::Data<Arc<AppState>> = web::Data::new(state);

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(actix_web::middleware::Logger::default()) // enable logger
            // .service(web::scope("").wrap(error_handlers()))
            .service(
                web::resource("/v1/communication_layer")
                    .route(web::get().to(v1_communication_layer)),
            )
            .service(web::resource("/v1/process").route(web::post().to(v1_process)))
    })
    .bind(("127.0.0.1", 9417))?
    .workers(8)
    .run()
    .await
    .map_err(anyhow::Error::from)
}
