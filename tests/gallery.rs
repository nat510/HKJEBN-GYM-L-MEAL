use gyml::{model::*, parser::*};

#[test]
fn coupon_links_keep_name_unit_price_and_reject_external_targets() {
    let html = r#"<div id='coupon_div'><div class='prom exchange-tw' onclick="window.open('download.php?id=12')"><div class='prod-name'><span>每袋</span>急凍美國牛</div><div class='price'>$99</div></div><div onclick="window.open('https://evil.test/x')">Bad</div></div>"#;
    let links = gallery_links(html, GalleryKind::Coupon);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].title, "急凍美國牛");
    assert_eq!(links[0].unit, "每袋");
    assert_eq!(links[0].price_text, "$99");
    assert_eq!(links[0].kind, "exchange");
}

#[test]
fn gallery_uses_full_size_image_and_navigation_label_without_tracking_images() {
    let html = r#"<div id='promotion_nav'><a href='#sale'>肉類推廣</a></div><div class='promotion_item' id='sale'><a href='/assets/full.jpg'><img src='/assets/thumb.jpg'></a><img src='/assets/icon.png'><img src='https://evil.test/tracker.jpg'></div>"#;
    let items = parse_gallery(html, GalleryKind::Promotion);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "肉類推廣");
    assert_eq!(items[0].image_url, "https://www.hkjebn.com/assets/full.jpg");
}

#[test]
fn latest_filter_excludes_unchanged_prices_and_never_invents_a_year() {
    let products = parse_products(include_str!("fixtures/meat.html"), Category::Meat).unwrap();
    let prefs = Preferences {
        latest: true,
        ..Default::default()
    };
    let ids: Vec<_> = select_products(&products, &prefs)
        .into_iter()
        .map(|i| products[i].id.as_str())
        .collect();
    assert_eq!(ids, vec!["88123063"]);
    assert_eq!(latest_batch(&products), Some((false, "10月8日生效".into())));
    assert_eq!(
        effective_date("2024年2月29日生效"),
        Some("2024-02-29".into())
    );
    assert_eq!(effective_date("2025年2月29日生效"), None);
}
