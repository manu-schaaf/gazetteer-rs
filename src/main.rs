#![feature(is_some_with)]

#[macro_use]
extern crate rocket;

use std::borrow::Cow;

use rocket::{form, State};
use rocket::form::{Context, Contextual, Error, Form, FromForm};
use rocket::fs::{FileServer, relative, TempFile};
use rocket::http::Status;
use rocket::serde::{Deserialize, Serialize};
use rocket::serde::json::{Json, Value};
use rocket::serde::json::serde_json::json;
use rocket_dyn_templates::{context, Template};

use gazetteer::tree::{MultiTree, ResultSelection, SearchTree};
use gazetteer::util::read_lines;

#[cfg(test)]
mod rocket_test;

#[derive(Debug, FromForm)]
struct Submit<'v> {
    text: &'v str,
    file: TempFile<'v>,
    max_len: usize,
    result_selection: ResultSelection,
}


#[derive(Debug, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Request<'r> {
    text: Cow<'r, str>,
    max_len: Option<usize>,
    result_selection: Option<Cow<'r, str>>,
}

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


#[get("/")]
fn index() -> Template {
    Template::render("index", &Context::default())
}

#[post("/", data = "<form>")]
fn submit<'r>(mut form: Form<Contextual<'r, Submit<'r>>>, tree: &State<MultiTree>) -> (Status, Template) {
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

#[post("/tag", format = "json", data = "<request>")]
async fn tag(
    request: Json<Request<'_>>,
    tree: &State<MultiTree>,
) -> Value {
    let result_selection = match &request.result_selection {
        Some(sel) => match sel.as_ref() {
            "All" => &ResultSelection::All,
            "Last" => &ResultSelection::Last,
            "Longest" => &ResultSelection::Longest,
            _ => {
                println!("Unknown result selection method '{}', defaulting to 'Longest'", sel);
                &ResultSelection::Longest
            }
        },
        None => &ResultSelection::Longest
    };
    let results = tree.search(
        &request.text,
        request.max_len.or_else(|| Some(5 as usize)),
        Option::from(result_selection),
    );
    json!({
        "status": "ok",
        "results": results
    })
}

#[catch(500)]
fn tag_error() -> Value {
    json!({
        "status": "error",
        "reason": "An error occurred during tree search."
    })
}

#[launch]
fn rocket() -> _ {
    let mut filter_list = read_lines("resources/filter_de.txt");
    filter_list.sort_unstable();

    let mut tree = MultiTree::default();
    // tree.add_balanced("resources/taxa/_old/taxa_2019_09_27.txt", false, true, Option::from(&filter_list));
    tree.add_balanced("resources/taxon/*.list", false, true, true, Option::from(&filter_list));
    tree.add_balanced("resources/vernacular/*.list", false, false, false, Option::from(&filter_list));

    println!("Fininshed loading gazetteer.");

    rocket::build()
        .mount("/", routes![index, submit, tag])
        .register("/tag", catchers![tag_error])
        .attach(Template::fairing())
        .mount("/", FileServer::from(relative!("/static")))
        .manage(tree)
}

// fn main() {
//     println!("Hello World")
//     // let (tree, symspell) = util::load_symspell("resources/taxa/Lichen/".to_string(), "resources/de-100k.txt");
//     // let string = String::from("Lyronna dolichobellum abc abc").to_lowercase();
//     // println!("{:?}", tree.traverse(string.clone().split(' ').collect::<VecDeque<&str>>()));
//     // let results = symspell.lookup_compound(string.as_str(), 2);
//     // if results.len() > 0 {
//     //     println!("{}", results[0].term);
//     //     println!("{:?}", tree.traverse(results[0].term.split(' ').collect::<VecDeque<&str>>()));
//     // }
// }