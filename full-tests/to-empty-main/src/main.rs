use std::collections::HashMap;

/// ~REQUIRE-DELETED user-fn
fn user(mut map: HashMap<(), ()>) {
    map.insert((), ());
}

/// ~MINIMIZE-ROOT main
fn main() {
    let map = HashMap::new();
    user(map);
    "~REQUIRE-DELETED main-body";
}
