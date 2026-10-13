use gyml::{model::*, parser::*};

#[test]
fn real_source_keeps_full_names_and_labelled_prices() {
    let meat = parse_products(include_str!("fixtures/meat.html"), Category::Meat).unwrap();
    assert_eq!(meat.len(), 33);
    let beef = meat.iter().find(|p| p.id == "88123027").unwrap();
    assert_eq!(beef.name, "急凍美國特級牛小排");
    assert_eq!(beef.prices.selling, Some(219.0));
    assert_eq!(beef.prices.member, Some(208.0));
    assert_eq!(beef.unit, "價錢每袋 (1磅)");
    let fish = parse_products(include_str!("fixtures/seafood.html"), Category::Seafood).unwrap();
    assert_eq!(fish.len(), 29);
    assert!(fish.iter().all(|p| !p.name.ends_with("...")));
}

#[test]
fn current_prices_do_not_swallow_future_price_batches() {
    let meat = parse_products(include_str!("fixtures/meat.html"), Category::Meat).unwrap();
    let beef = meat.iter().find(|p| p.id == "88123063").unwrap();
    assert_eq!(beef.prices.selling, Some(69.0));
    assert_eq!(beef.prices.member, Some(66.0));
    assert_eq!(beef.price_batches[0].prices.member, Some(63.0));
    assert_eq!(beef.price_batches[0].raw_date, "10月8日生效");
    assert_eq!(beef.price_batches[0].date_iso, None);
}

#[test]
fn parser_rejects_partial_product_rows_and_untrusted_links() {
    for html in [
        "<a href='product-detail.php?product_code=1'><div class='prod-name'>牛...</div><div class='prod-price'>售價 HKD 20</div></a>",
        "<a href='product-detail.php?product_code=1'><div class='prod-name'>牛</div></a>",
        "<a href='https://evil.test/product-detail.php?product_code=1'><div class='prod-name'>牛</div><div class='prod-price'>售價 HKD 20</div></a>",
    ] {
        assert!(parse_products(html, Category::Meat).is_err());
    }
}

#[test]
fn queue_response_is_not_an_empty_catalogue() {
    assert!(check_html(200, include_str!("fixtures/queue.html"), true).is_err());
    assert!(check_html(200, "<html>maintenance</html>", true).is_err());
    assert!(check_html(429, "", true).is_err());
}

#[test]
fn default_filters_hide_expensive_and_excluded_and_price_sort_uses_member() {
    let products = vec![
        product("1", "美國牛", 20.0, Promotion::Like),
        product("2", "海參", 5.0, Promotion::Like),
        product("3", "中國魚", 6.0, Promotion::Regular),
        product("4", "日本魚", 10.0, Promotion::Special),
    ];
    let prefs = Preferences {
        sort: SortMode::PriceAsc,
        ..Default::default()
    };
    assert_eq!(select_products(&products, &prefs), vec![3, 0]);
    let prefs = Preferences {
        search: "美國".into(),
        ..Default::default()
    };
    assert_eq!(select_products(&products, &prefs), vec![0]);
    assert_eq!(
        discount(&Prices {
            original: Some(100.0),
            member: Some(80.0),
            selling: Some(90.0)
        }),
        Some(20.0)
    );
}

fn product(id: &str, name: &str, price: f64, promotion: Promotion) -> Product {
    Product {
        id: id.into(),
        name: name.into(),
        categories: vec![Category::Meat],
        unit: String::new(),
        image: None,
        url: format!("https://mp.hkjebn.com/product-detail.php?product_code={id}"),
        promotion,
        promotion_labels: vec![],
        prices: Prices {
            member: Some(price),
            ..Default::default()
        },
        price_batches: vec![],
    }
}

#[test]
fn default_sort_places_changed_regular_prices_before_unchanged_regular() {
    let mut products = vec![
        product("1", "牛一", 20.0, Promotion::Regular),
        product("2", "牛二", 30.0, Promotion::Regular),
    ];
    products[1].price_batches.push(PriceBatch {
        raw_date: "10月8日生效".into(),
        date_iso: None,
        unchanged: false,
        prices: Prices {
            member: Some(25.0),
            ..Default::default()
        },
    });
    assert_eq!(
        select_products(&products, &Preferences::default()),
        vec![1, 0]
    );
}

#[test]
fn incomplete_labelled_price_rejects_the_entire_page() {
    let html = include_str!("fixtures/meat.html").replacen("HKD&nbsp;208.00", "HKD&nbsp;", 1);
    assert!(parse_products(&html, Category::Meat).is_err());
    let html = include_str!("fixtures/meat.html").replacen("HKD&nbsp;63.00", "HKD&nbsp;", 1);
    assert!(parse_products(&html, Category::Meat).is_err());
}

#[test]
fn missing_product_href_rejects_page_instead_of_dropping_product() {
    let html = include_str!("fixtures/meat.html").replacen(
        "href=\"product-detail.php?product_code=88123027&amp;lang=tw&amp;region=hk\"",
        "",
        1,
    );
    assert!(parse_products(&html, Category::Meat).is_err());
}

#[test]
fn capture_time_is_shown_in_japan_time() {
    assert_eq!(captured_time(0), "1970/01/01 09:00");
    assert_eq!(captured_time(1704034800), "2024/01/01 00:00");
}
