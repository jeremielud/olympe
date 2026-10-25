fn main() {
    let c = zeus::sg::Catalog::load(std::path::Path::new(&std::env::args().nth(1).unwrap())).unwrap();
    for (i, b) in c.bitmaps.iter().enumerate() {
        println!("{i:3} {}", b.name);
    }
}
