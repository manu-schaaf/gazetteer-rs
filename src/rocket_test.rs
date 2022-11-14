use rocket::http::Status;
use rocket::local::blocking::Client;
use rocket::serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(crate = "rocket::serde")]
struct Request {
    text: String,
    max_len: Option<usize>,
    result_selection: Option<String>,
}

impl Request {
    fn new(text: impl Into<String>) -> Self {
        Request {
            text: text.into(),
            max_len: None,
            result_selection: None,
        }
    }
}

#[test]
fn json_tag() {
    let client = Client::tracked(super::rocket()).unwrap();

    let message = Request::new("Nach Schluß des Congresses ist eine längere Excursion vorgesehen, auf welcher die Inseln an der Küste von Pembrokshire besucht werden.
Dieser Ausflug dürfte besonders interessant werden, weil sich hier große Brutkolonien von Puffinus p. puffinus und verschiedener Alcidae befinden.
Auch Thalassidroma pelagica dürfte hier angetroffen werden.
Bei günstigem Wetter ist ferner der Besuch einer Brutkolonie von Sula bassana vorgesehen.");
    let res = client.post("/v1/process").json(&message).dispatch();
    assert_eq!(res.status(), Status::Ok);
    println!("{:?}", res.body())
}
