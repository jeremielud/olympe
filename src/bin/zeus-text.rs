//! Affiche un groupe de textes de `zeus_text.eng`, une chaîne numérotée par ligne.

use zeus::text::Text;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(file), Some(group)) = (args.get(1), args.get(2).and_then(|g| g.parse::<usize>().ok())) else {
        eprintln!("usage : zeus-text <zeus_text.eng> <groupe> [nombre de chaînes]");
        std::process::exit(2);
    };
    let count = args.get(3).and_then(|c| c.parse().ok()).unwrap_or(40);
    let text = std::fs::read(file).map_err(|e| e.to_string()).and_then(|data| Text::parse(&data));
    match text {
        Ok(text) => (0..count).for_each(|index| println!("{index} {}", text.get(group, index))),
        Err(e) => {
            eprintln!("erreur : {e}");
            std::process::exit(1);
        }
    }
}
