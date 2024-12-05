use std::{collections::HashMap, sync::Arc};

use lazy_static::lazy_static;
use serde::Deserialize;

use actix_web::{web, HttpResponse};
use tera::{Context, Tera};

use crate::{tree::ResultSelection, AppState};

lazy_static! {
    pub static ref TEMPLATES: Tera = {
        let mut tera = match Tera::new("src/templates/**/*") {
            Ok(t) => t,
            Err(e) => {
                println!("Parsing error(s): {}", e);
                ::std::process::exit(1);
            }
        };
        tera.autoescape_on(vec![".html"]);
        tera
    };
}

pub async fn index() -> HttpResponse {
    let mut context = Context::new();
    let errors: Vec<String> = Vec::new();
    let values: HashMap<String, String> = HashMap::new();
    context.insert("errors", &errors);
    context.insert("values", &values);
    let body = TEMPLATES
        .render("index.html.tera", &context)
        .expect("Failed to render template!");
    HttpResponse::Ok().body(body)
}

#[derive(Deserialize, Debug)]
pub struct FormData {
    text: String,
    max_len: Option<usize>,
    result_selection: Option<ResultSelection>,
}

pub async fn process_form(
    form: web::Form<FormData>,
    state: web::Data<Arc<AppState>>,
) -> HttpResponse {
    let results: &Vec<(String, Vec<crate::tree::Match>, usize, usize)> =
        &state
            .tree
            .search(&form.text, form.max_len, form.result_selection.as_ref());

    let mut context = Context::new();
    context.insert("results", results);
    let body = Tera::one_off(include_str!("templates/success.html.tera"), &context, false)
        .expect("Failed to render template");
    HttpResponse::Ok().body(body)
}
