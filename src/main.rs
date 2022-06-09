#![feature(is_some_with)]
#[macro_use]
extern crate rocket;

use rocket::{form, State};
use rocket::form::{Context, Contextual, Error, Form, FromForm};
use rocket::fs::{FileServer, relative, TempFile};
use rocket::http::Status;
use rocket_dyn_templates::{context, Template};

use gazetteer::tree::{MultiTree, ResultSelection, SearchTree};
use gazetteer::util::read_lines;

#[derive(Debug, FromForm)]
struct Submit<'v> {
    text: &'v str,
    file: TempFile<'v>,
    max_len: usize,
    result_selection: ResultSelection,
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

// NOTE: We use `Contextual` here because we want to collect all submitted form
// fields to re-render forms with submitted values on error. If you have no such
// need, do not use `Contextual`. Use the equivalent of `Form<Submit<'_>>`.
#[post("/", data = "<form>")]
fn submit<'r>(mut form: Form<Contextual<'r, Submit<'r>>>, tree: &State<MultiTree>) -> (Status, Template) {
    let template = match form.value {
        Some(ref submission) => {
            println!("submission: {:#?}", submission);
            match file_or_text(submission.text, &submission.file) {
                Ok(text) => {
                    let results = tree.search(&text, Option::from(submission.max_len), Option::from(&submission.result_selection));
                    for result in results.iter() {
                        println!("{:?} ({},{}) -> {:?}", result.0, result.2, result.2, result.1)
                    }
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


// #[post("/tag")]
// fn tag(
//     text: String,
//     max_len: Option<usize>,
//     result_selection: Option<&str>,
//     tree: &State<MultiTree>,
// ) -> Result<Vec<(String, String, (usize, usize))>, rocket::Error> {
//     let result_selection = Option::from(match result_selection.unwrap_or("longest").to_lowercase().as_str() {
//         "all" => &ResultSelection::All,
//         "last" => &ResultSelection::Last,
//         _ => &ResultSelection::Longest,
//     });
//     Ok(tree.search(&text, max_len, result_selection))
// }

#[launch]
fn rocket() -> _ {
    let mut filter_list = read_lines("resources/filter_de.txt");
    filter_list.sort_unstable();

    let mut tree = MultiTree::default();
    // tree.add_balanced("resources/taxa/_old/taxa_2019_09_27.txt", false, true, Option::from(&filter_list));
    tree.add_balanced("resources/taxa/_current/taxon/*.list", false, true, Option::from(&filter_list));
    tree.add_vernacular("resources/taxa/_current/vernacular/*.list", Option::from(&filter_list));

    rocket::build()
        .mount("/", routes![index, submit])
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