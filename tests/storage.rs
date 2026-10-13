use gyml::{model::*, parser::parse_products, storage::*};
use std::fs;

#[test]
fn corrupt_cache_is_rejected_and_invalid_update_preserves_previous() {
    let dir = std::env::temp_dir().join(format!("gyml-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut products = parse_products(include_str!("fixtures/meat.html"), Category::Meat).unwrap();
    products
        .extend(parse_products(include_str!("fixtures/seafood.html"), Category::Seafood).unwrap());
    let catalog = Catalog {
        version: 1,
        fetched_at: 1720000000,
        products: deduplicate(products),
    };
    save_catalog(&dir, &catalog).unwrap();
    let old_bytes = fs::read(dir.join("catalog.json")).unwrap();
    let mut invalid = catalog.clone();
    invalid.products.clear();
    assert!(save_catalog(&dir, &invalid).is_err());
    assert_eq!(fs::read(dir.join("catalog.json")).unwrap(), old_bytes);
    assert_eq!(
        load_catalog(&dir).unwrap().products.len(),
        catalog.products.len()
    );
    fs::write(dir.join("catalog.json"), "{bad json").unwrap();
    assert!(load_catalog(&dir).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn untrusted_image_and_duplicate_product_ids_fail_validation() {
    let mut products = parse_products(include_str!("fixtures/meat.html"), Category::Meat).unwrap();
    products
        .extend(parse_products(include_str!("fixtures/seafood.html"), Category::Seafood).unwrap());
    let mut catalog = Catalog {
        version: 1,
        fetched_at: 1720000000,
        products: deduplicate(products),
    };
    catalog.products[0].image = Some("https://evil.test/private".into());
    assert!(catalog.validate().is_err());
    catalog.products[0].image = None;
    catalog.products.push(catalog.products[0].clone());
    assert!(catalog.validate().is_err());
}

#[test]
fn portable_directory_depends_on_executable_location() {
    assert_eq!(
        data_dir_for(std::path::Path::new("C:/USB/gymL.exe")).unwrap(),
        std::path::PathBuf::from("C:/USB/gymL_data")
    );
}
