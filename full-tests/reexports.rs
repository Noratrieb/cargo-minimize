
use hello::{thingy, whatever};
mod hello{
    pub fn thingy(){}
    /// ~REQUIRE-DELETED whatever
    pub fn whatever(){}
}

fn main(){
    "~MINIMIZE-ROOT let x = thingy";
    let x = thingy();
}
