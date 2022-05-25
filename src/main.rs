use std::collections::VecDeque;

fn main() {
    let mut tree = StringTree::new("", "".parse().unwrap());
    for (s, uri) in vec![("An example phrase", "uri:phrase"), ("An example", "uri:example")] {
        let s = String::from(s);
        let uri = String::from(uri);
        let v: VecDeque<&str> = s.split(' ').collect::<VecDeque<&str>>();
        tree.insert(v, uri);
    }
    println!("{:?}", tree.traverse(String::from("An xyz").split(' ').collect::<VecDeque<&str>>()));
    println!("{:?}", tree.traverse(String::from("An example").split(' ').collect::<VecDeque<&str>>()));
    println!("{:?}", tree.traverse(String::from("An example phrase").split(' ').collect::<VecDeque<&str>>()));
}

struct StringTree {
    value: String,
    uri: String,
    children: Vec<StringTree>,
}


impl StringTree {
    fn new(value: &str, uri: String) -> Self {
        let value = String::from(value);
        Self {
            value,
            uri,
            children: vec![],
        }
    }

    fn get_value(&self) -> &String {
        &self.value
    }

    fn insert(&mut self, mut values: VecDeque<&str>, uri: String) -> bool {
        let value = values.pop_front().unwrap();
        match self.children.binary_search_by_key(&value, |a| a.get_value()) {
            Ok(idx) => {
                if values.is_empty() {
                    if self.children[idx].uri.is_empty() {
                        self.children[idx].uri = uri;
                        true
                    } else {
                        false
                    }
                } else {
                    self.children[idx].insert(values, uri)
                }
            }
            Err(idx) => {
                if values.is_empty() {
                    self.children.insert(idx, StringTree::new(value, uri));
                    true
                } else {
                    self.children.insert(idx, StringTree::new(value, String::new()));
                    self.children[idx].insert(values, uri)
                }
            }
        }
    }

    pub fn traverse(&self, values: VecDeque<&str>) -> Result<(&String, i32), (&String, i32)> {
        self._traverse(values, 1)
    }

    fn _traverse(&self, mut values: VecDeque<&str>, counter: i32) -> Result<(&String, i32), (&String, i32)> {
        let value = values.pop_front().expect("");
        match self.children.binary_search_by_key(&value, |a| a.get_value()) {
            Ok(idx) => {
                if values.is_empty() {
                    if self.children[idx].uri.is_empty() {
                        Err((&self.children[idx].uri, counter))
                    } else {
                        Ok((&self.children[idx].uri, counter))
                    }
                } else {
                    self.children[idx]._traverse(values, counter + 1)
                }
            }
            Err(_) => {
                Err((&self.uri, counter))
            }
        }
    }
}