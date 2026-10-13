use gyml::{fetch::*, model::*, parser::parse_products};
use url::Url;

#[test]
fn pagination_requires_an_empty_terminator_and_rejects_repeated_pages() {
    let rows = parse_products(include_str!("fixtures/meat.html"), Category::Meat).unwrap();
    let good = collect_pages(|page| Ok(if page == 1 { rows.clone() } else { vec![] }), 20).unwrap();
    assert_eq!(good.len(), 33);
    assert!(collect_pages(|_| Ok(rows.clone()), 20).is_err());
    assert!(collect_pages(|_| Ok(vec![]), 20).is_err());
    assert!(collect_pages(|_| Ok(rows.clone()), 1).is_err());
}

#[test]
fn redirect_cannot_change_product_category_or_send_to_another_domain() {
    let original =
        Url::parse("https://mp.hkjebn.com/product.php?cat=1408&subcat=1409&mode=detail").unwrap();
    for u in [
        "http://mp.hkjebn.com/product.php?cat=1408&subcat=1409&mode=detail",
        "https://evil.test/",
        "https://mp.hkjebn.com/product.php?cat=1408&subcat=1620&mode=detail",
        "https://user:pass@mp.hkjebn.com/product.php?cat=1408&subcat=1409&mode=detail",
    ] {
        assert!(!allowed_redirect(&Url::parse(u).unwrap(), &original));
    }
    assert!(allowed_redirect(
        &Url::parse("https://hkjebn.queue-it.net/?c=hkjebn").unwrap(),
        &original
    ));
    assert!(allowed_redirect(
        &Url::parse(
            "https://mp.hkjebn.com/product.php?cat=1408&subcat=1409&mode=detail&queueittoken=abc"
        )
        .unwrap(),
        &original
    ));
}
