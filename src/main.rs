use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use clap::{arg, Parser};

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
            let mut value: HashMap<(String, String), Vec<String>> = HashMap::new();
            for mtch in mtches {
                value
                    .entry((mtch.match_string.to_string(), mtch.match_type.to_string()))
                    .and_modify(|e| e.push(mtch.match_label.to_string()))
                    .or_insert_with(|| vec![mtch.match_label.to_string()]);
            }

            let ((match_strings, match_types), match_labels): (
                (Vec<String>, Vec<String>),
                Vec<String>,
            ) = value
                .into_iter()
                .map(|((s, t), l)| ((s, t), l.join(" ")))
                .unzip();
            json!({
                "string": string,
                "match_labels": match_labels.join(" | "),
                "match_types": match_types.join(" | "),
                "match_strings": match_strings.join(" | "),
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

fn parse_args_and_build_tree(config_path: &str) -> anyhow::Result<HashMapSearchTree> {
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
    // #[arg(long, default_value_t = 536_870_912)]
    #[arg(long, default_value_t = 16_777_216, help = "The request size limit")]
    limit: usize,
}

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    env_logger::init_from_env(env_logger::Env::new().default_filter_or(LOG_LEVEL));

    let json_config = web::JsonConfig::default().limit(args.limit);

    let state: Arc<AppState> = Arc::new(AppState {
        tree: parse_args_and_build_tree(&args.config)?,
    });
    let data: web::Data<Arc<AppState>> = web::Data::new(state);

    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(actix_web::middleware::Logger::default())
            .wrap(actix_web::middleware::Compress::default())
            .wrap(actix_web::middleware::DefaultHeaders::default())
            .app_data(json_config.clone())
            // .service(web::scope("").wrap(error_handlers()))
            .service(
                web::resource("/v1/communication_layer")
                    .route(web::get().to(v1_communication_layer)),
            )
            .service(web::resource("/v1/process").route(web::post().to(v1_process)))
    })
    .bind((args.address, args.port))?
    .workers(args.workers)
    .run()
    .await
    .map_err(anyhow::Error::from)
}
