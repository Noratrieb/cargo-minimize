use std::collecions::HashMap;

/// ~REQUIRE-DELETED
fn user(map: HashMap<(), ()>) {
    map.insert((), ());
}

/// ~MINIMIZE-ROOT main
fn main() {
    let map = HashMap::new();
    user(map);
}
