use gyml::{fetch::*, model::*, storage::*};
fn main() {
    let dir = std::env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("tests/output/live"));
    match refresh_catalog(|s| println!("{s}")) {
        Ok(c) => {
            println!("Catalogue: {} products", c.products.len());
            if let Err(e) = save_catalog(&dir, &c) {
                eprintln!("{e}");
            }
        }
        Err(e) => eprintln!("Catalogue: {e}"),
    }
    for kind in [GalleryKind::Coupon, GalleryKind::Promotion] {
        match refresh_gallery(kind, |s| println!("{s}")) {
            Ok(g) => {
                println!("{}: {} items", kind.label(), g.items.len());
                if let Err(e) = save_gallery(&dir, kind, &g) {
                    eprintln!("{e}");
                }
            }
            Err(e) => eprintln!("{}: {e}", kind.label()),
        }
    }
}
