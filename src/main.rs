use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use anyhow::Context;
use clap::{arg, Parser};

use actix_files as fs;
use actix_web::{web, App, HttpServer};

use gazetteer::api;
use gazetteer::tree::HashMapSearchTree;
use gazetteer::util::{read_lines, CorpusFormat};
use gazetteer::AppState;

#[cfg(feature = "gui")]
use gazetteer::gui;

const DEFAULT_GENERATE_ABBRV: bool = false;
const DEFAULT_ABBRV_MAX_INDEX: i32 = 1;
const DEFAULT_ABBRV_MIN_SUFFIX_LENGTH: i32 = 3;
const DEFAULT_GENERATE_SKIP_GRAMS: bool = false;
const DEFAULT_SKIP_GRAM_MAX_SKIPS: i32 = 2;
const DEFAULT_SKIP_GRAM_MIN_LENGTH: i32 = 2;

#[cfg(debug_assertions)]
const LOG_LEVEL: &str = "debug";
#[cfg(not(debug_assertions))]
const LOG_LEVEL: &str = "info";

#[derive(Serialize, Deserialize)]
struct Config {
    filter_path: Option<String>,
    generate_abbrv: Option<bool>,
    abbrv_max_index: Option<i32>,
    abbrv_min_suffix_length: Option<i32>,
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
    abbrv_max_index: Option<i32>,
    abbrv_min_suffix_length: Option<i32>,
    generate_skip_grams: Option<bool>,
    skip_gram_min_length: Option<i32>,
    skip_gram_max_skips: Option<i32>,
    format: Option<CorpusFormat>,
}

fn parse_args_and_build_tree(config_path: &str) -> anyhow::Result<HashMapSearchTree> {
    let config: String =
        std::fs::read_to_string(config_path).context("Failed to load configuration.")?;

    let config: Config = toml::from_str(&config).context("Failed to parse configuration TOML")?;

    let mut tree = HashMapSearchTree::default();
    let default_filter_list = load_filter_list(config.filter_path);

    for corpus in config.corpora.values() {
        let root_path: &String = &corpus.path;
        let generate_abbrv = corpus
            .generate_abbrv
            .unwrap_or_else(|| config.generate_abbrv.unwrap_or(DEFAULT_GENERATE_ABBRV));
        let abbrv_max_index = corpus
            .abbrv_max_index
            .unwrap_or_else(|| config.abbrv_max_index.unwrap_or(DEFAULT_ABBRV_MAX_INDEX));
        let abbrv_min_suffix_length = corpus.abbrv_min_suffix_length.unwrap_or_else(|| {
            config
                .abbrv_min_suffix_length
                .unwrap_or(DEFAULT_ABBRV_MIN_SUFFIX_LENGTH)
        });
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
                root_path,
                generate_skip_grams,
                skip_gram_min_length,
                skip_gram_max_skips,
                &filter_list,
                generate_abbrv,
                abbrv_max_index,
                abbrv_min_suffix_length,
                format,
            );
        } else {
            tree.load_file(
                root_path,
                generate_skip_grams,
                skip_gram_min_length,
                skip_gram_max_skips,
                &default_filter_list,
                generate_abbrv,
                abbrv_max_index,
                abbrv_min_suffix_length,
                format,
            );
        }
    }
    println!(
        "Finished loading gazetteer with {} entries",
        tree.search_map.len()
    );
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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value_t = String::from("config.toml"))]
    config: String,
    #[arg(short, long, default_value_t = String::from("0.0.0.0"))]
    address: String,
    #[arg(short, long, default_value_t = 9714)]
    port: u16,
    #[arg(short, long, default_value_t = 1)]
    workers: usize,
    #[arg(long, default_value_t = 16_777_216, help = "The request size limit")]
    limit: usize,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let accept_all = |_| true;
    let json_config = web::JsonConfig::default()
        .content_type_required(false)
        .content_type(accept_all)
        .limit(args.limit);

    env_logger::init_from_env(env_logger::Env::new().default_filter_or(LOG_LEVEL));

    let state: Arc<AppState> = Arc::new(AppState {
        tree: parse_args_and_build_tree(&args.config)?,
    });
    let data: web::Data<Arc<AppState>> = web::Data::new(state);

    HttpServer::new(move || {
        let app = App::new()
            .app_data(data.clone())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(actix_web::middleware::Compress::default())
            .app_data(json_config.clone())
            .service(
                web::resource("/v1/process")
                    .wrap(
                        actix_web::middleware::DefaultHeaders::default()
                            .add(("Content-Type", "application/json")),
                    )
                    .route(web::post().to(api::v1_process)),
            )
            .service(
                web::resource("/v1/communication_layer")
                    .route(web::get().to(api::v1_communication_layer)),
            );

        #[cfg(feature = "gui")]
        let app = {
            app.service(
                fs::Files::new("/static", "src/static/")
                    .show_files_listing()
                    .use_last_modified(true),
            )
            .service(
                web::resource("/")
                    .route(web::get().to(gui::index))
                    .route(web::post().to(gui::process_form)),
            )
        };

        app
    })
    .bind((args.address, args.port))?
    .workers(args.workers)
    .run()
    .await
    .map_err(anyhow::Error::from)
}
