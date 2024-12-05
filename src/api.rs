use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use actix_files::NamedFile;
use actix_web::web;
use actix_web::HttpResponse;
use actix_web::Result;

use crate::tree::ResultSelection;
use crate::util::parse_optional;
use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessRequest<'r> {
    pub text: Cow<'r, str>,
    pub max_len: Option<String>,
    pub result_selection: Option<ResultSelection>,
}

pub async fn v1_communication_layer() -> Result<NamedFile> {
    Ok(NamedFile::open_async("communication_layer.lua").await?)
}

pub async fn v1_process(
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
