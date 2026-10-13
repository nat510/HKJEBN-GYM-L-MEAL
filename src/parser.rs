use crate::model::*;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use std::{collections::HashSet, sync::LazyLock};
use url::Url;

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("constant selector")
}
pub fn clean(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn text(e: ElementRef<'_>) -> String {
    clean(&e.text().collect::<Vec<_>>().join(" "))
}
fn first_text(e: ElementRef<'_>, selector: &Selector) -> String {
    e.select(selector).next().map(text).unwrap_or_default()
}

static PRICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(原價|售價|會員價)\s*[:：]?\s*(?:HKD|HK\$)\s*(\d+(?:,\d{3})*(?:\.\d{1,2})?)")
        .unwrap()
});
static TOKENS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)((?:\d{4}\s*(?:年|[-/.])\s*)?\d{1,2}\s*(?:月|[-/.])\s*\d{1,2}(?:日)?\s*生效)|(價目不變)|(原價|售價|會員價)\s*[:：]?\s*(?:HKD|HK\$)\s*(\d+(?:,\d{3})*(?:\.\d{1,2})?)").unwrap()
});
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(\d{4})\s*(?:年|[-/.])\s*(\d{1,2})\s*(?:月|[-/.])\s*(\d{1,2})(?:日)?").unwrap()
});

fn set_price(prices: &mut Prices, label: &str, amount: &str) {
    if let Ok(v) = amount.replace(',', "").parse::<f64>() {
        match label {
            "原價" => prices.original = Some(v),
            "售價" => prices.selling = Some(v),
            "會員價" => prices.member = Some(v),
            _ => {}
        }
    }
}
fn validate_price_labels(raw: &str) -> Result<(), String> {
    static LABEL: LazyLock<Regex> = LazyLock::new(|| Regex::new("原價|售價|會員價").unwrap());
    if LABEL.find_iter(raw).count() != PRICE.captures_iter(raw).count() {
        return Err("來源價格欄位不完整；停止本次更新".into());
    }
    Ok(())
}
pub fn effective_date(raw: &str) -> Option<String> {
    let cap = DATE.captures(raw)?;
    let y: u32 = cap[1].parse().ok()?;
    let m: usize = cap[2].parse().ok()?;
    let d: u32 = cap[3].parse().ok()?;
    if y < 1000 || !(1..=12).contains(&m) {
        return None;
    }
    let days = [
        31,
        if y.is_multiple_of(400) || y.is_multiple_of(4) && !y.is_multiple_of(100) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if d == 0 || d > days[m - 1] {
        return None;
    }
    Some(format!("{y:04}-{m:02}-{d:02}"))
}
pub fn parse_prices(raw: &str) -> (Prices, Vec<PriceBatch>) {
    let mut current = Prices::default();
    let mut batches: Vec<PriceBatch> = vec![];
    for c in TOKENS.captures_iter(raw) {
        if let Some(date) = c.get(1) {
            let raw_date = clean(date.as_str());
            batches.push(PriceBatch {
                date_iso: effective_date(&raw_date),
                raw_date,
                unchanged: false,
                prices: Prices::default(),
            });
        } else if c.get(2).is_some() {
            if let Some(b) = batches.last_mut() {
                b.unchanged = true;
            }
        } else {
            let target = if let Some(b) = batches.last_mut() {
                &mut b.prices
            } else {
                &mut current
            };
            set_price(target, &c[3], &c[4]);
        }
    }
    (current, batches)
}

pub fn parse_products(html: &str, category: Category) -> Result<Vec<Product>, String> {
    let doc = Html::parse_fragment(html);
    let rows = sel("a[href*='product-detail.php']");
    let name_sel = sel(".prod-name");
    let unit_sel = sel(".prod-des");
    let price_sel = sel(".prod-price");
    let preview_sel = sel(".price-preview");
    let image_sel = sel(".thumb");
    let label_sel = sel("[class^='discount_label']");
    let containers = doc.select(&sel(".product_item_detail")).count();
    if containers > 0 && doc.select(&rows).count() != containers {
        return Err("來源有商品缺少有效連結；停止本次更新".into());
    }
    let mut out = vec![];
    for row in doc.select(&rows) {
        let raw = row.value().attr("href").unwrap_or("");
        let url = retail_url(raw, category.source()).ok_or("來源商品連結不合法")?;
        let parsed = Url::parse(&url).map_err(|_| "来源連結不合法")?;
        let id = parsed
            .query_pairs()
            .find(|(k, _)| k == "product_code")
            .map(|(_, v)| v.into_owned())
            .ok_or("來源缺少商品編號")?;
        if id.is_empty() || !id.bytes().all(|c| c.is_ascii_digit()) {
            return Err("來源商品編號不合法".into());
        }
        let name = first_text(row, &name_sel);
        let unit = first_text(row, &unit_sel);
        let mut prices = Prices::default();
        for e in row.select(&price_sel) {
            // Preview descendants must never contribute current prices.
            let raw = e
                .descendants()
                .filter_map(|node| {
                    if let scraper::node::Node::Text(t) = node.value() {
                        let preview = node
                            .ancestors()
                            .filter_map(ElementRef::wrap)
                            .any(|a| a.value().classes().any(|c| c == "price-preview"));
                        if !preview {
                            return Some(t.to_string());
                        }
                    }
                    None
                })
                .collect::<Vec<_>>()
                .join(" ");
            validate_price_labels(&raw)?;
            for c in PRICE.captures_iter(&raw) {
                set_price(&mut prices, &c[1], &c[2]);
            }
        }
        let preview = first_text(row, &preview_sel);
        validate_price_labels(&preview)?;
        let (_, price_batches) = parse_prices(&preview);
        let promotion_labels: Vec<_> = row
            .select(&label_sel)
            .map(text)
            .filter(|s| !s.is_empty())
            .collect();
        let normalized: Vec<_> = promotion_labels
            .iter()
            .map(|s| s.split_whitespace().collect::<String>().to_lowercase())
            .collect();
        let promotion = if normalized.iter().any(|s| s == "至like價") {
            Promotion::Like
        } else if normalized.iter().any(|s| s == "特價") {
            Promotion::Special
        } else if normalized.iter().any(|s| s == "正價")
            || normalized.iter().all(|s| s == "限" || s == "天天低價")
        {
            Promotion::Regular
        } else {
            Promotion::Unknown
        };
        if name.is_empty() || name.ends_with("...") || prices.empty() || !prices.valid() {
            return Err("來源商品名称不完整或缺少可辨識價格".into());
        }
        let image = row
            .select(&image_sel)
            .next()
            .and_then(|e| e.value().attr("src"))
            .and_then(|s| retail_url(s, category.source()));
        out.push(Product {
            id,
            name,
            categories: vec![category],
            unit,
            image,
            url,
            promotion,
            promotion_labels,
            prices,
            price_batches,
        });
    }
    Ok(out)
}

pub fn check_html(status: u32, html: &str, catalog: bool) -> Result<(), String> {
    if status != 200 {
        return Err(format!("來源 HTTP {status}；保留上次成功資料"));
    }
    let lower = html.to_ascii_lowercase();
    if [
        "queue-it",
        "queueittoken",
        "cookietest",
        "document.location.href = decodeuricomponent",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        return Err("來源要求排隊或互動驗證；請稍後重試".into());
    }
    if catalog && !html.contains("product_item_detail") && !html.contains("product-data.php") {
        return Err("來源商品頁面格式改變".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct GalleryLink {
    pub url: String,
    pub title: String,
    pub unit: String,
    pub price_text: String,
    pub kind: String,
}
pub fn gallery_links(html: &str, kind: GalleryKind) -> Vec<GalleryLink> {
    let doc = Html::parse_document(html);
    let source = kind.source();
    let targets = sel(if kind == GalleryKind::Coupon {
        "#coupon_div [onclick]"
    } else {
        "#promotion_nav a"
    });
    static CLICK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"window\.open\(\s*['"](download\.php\?id=\d+)['"]"#).unwrap()
    });
    let mut seen = HashSet::new();
    let mut out = vec![];
    for e in doc.select(&targets) {
        let raw = if kind == GalleryKind::Coupon {
            e.value()
                .attr("onclick")
                .and_then(|s| CLICK.captures(s))
                .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
        } else {
            e.value().attr("href").map(String::from)
        };
        let Some(raw) = raw.filter(|s| !s.starts_with('#')) else {
            continue;
        };
        let Some(url) = retail_url(&raw, source) else {
            continue;
        };
        let u = Url::parse(&url).unwrap();
        if u.origin() != Url::parse(source).unwrap().origin()
            || !(if kind == GalleryKind::Coupon {
                u.path() == "/download.php"
            } else {
                u.path().starts_with("/tw/hk/promotion/")
            })
            || !seen.insert(url.clone())
        {
            continue;
        }
        let unit = first_text(e, &sel(".prod-name span"));
        let title = if kind == GalleryKind::Coupon {
            let n = first_text(e, &sel(".prod-name"));
            n.strip_prefix(&unit).unwrap_or(&n).trim().to_string()
        } else {
            text(e)
        };
        out.push(GalleryLink {
            url,
            title,
            unit,
            price_text: first_text(e, &sel(".price")),
            kind: if e.value().classes().any(|s| s.contains("exchange")) {
                "exchange"
            } else {
                "surprise"
            }
            .into(),
        });
    }
    out
}

pub fn parse_gallery(html: &str, kind: GalleryKind) -> Vec<GalleryItem> {
    let doc = Html::parse_document(html);
    let source = kind.source();
    let targets = sel(if kind == GalleryKind::Coupon {
        "#coupon_div"
    } else {
        ".promotion_item"
    });
    let mut labels = HashMap::new();
    for e in doc.select(&sel("#promotion_nav a")) {
        if let Some(h) = e.value().attr("href").and_then(|s| s.strip_prefix('#')) {
            labels.insert(h.to_string(), text(e));
        }
    }
    let mut out = vec![];
    let mut seen = HashSet::new();
    static BACKGROUND: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)background(?:-image)?\s*:[^;]*?url\(\s*['"]?([^'"\s)]+)"#).unwrap()
    });
    for group in doc.select(&targets) {
        let mut title = group
            .value()
            .attr("id")
            .and_then(|id| labels.get(id))
            .cloned()
            .unwrap_or_else(|| {
                group
                    .value()
                    .attr("aria-label")
                    .or(group.value().attr("title"))
                    .unwrap_or("")
                    .into()
            });
        for e in std::iter::once(group).chain(group.select(&sel("*"))) {
            if e.value().name() == "img" && title.is_empty() {
                title = e.value().attr("alt").unwrap_or("").into();
            }
            let mut candidates = vec![];
            if e.value().name() == "img" {
                let src = ["data-src", "data-original", "data-lazy-src", "src"]
                    .iter()
                    .find_map(|a| e.value().attr(a));
                let full = e
                    .ancestors()
                    .filter_map(ElementRef::wrap)
                    .find_map(|a| a.value().attr("href").filter(|h| image_extension(h)));
                let srcset = e
                    .value()
                    .attr("data-srcset")
                    .or(e.value().attr("srcset"))
                    .and_then(|s| s.split(',').next_back())
                    .and_then(|s| s.split_whitespace().next());
                if let Some(s) = full.or(srcset).or(src) {
                    candidates.push(s.to_string());
                }
            }
            if let Some(style) = e.value().attr("style") {
                for c in BACKGROUND.captures_iter(style) {
                    candidates.push(c[1].into());
                }
            }
            for raw in candidates {
                let Some(image_url) = retail_url(&raw, source) else {
                    continue;
                };
                let path = Url::parse(&image_url).unwrap().path().to_ascii_lowercase();
                if [
                    "logo",
                    "icon",
                    "pixel",
                    "spacer",
                    "loading",
                    "spinner",
                    "close-btn",
                ]
                .iter()
                .any(|s| path.contains(s))
                    || !seen.insert(image_url.clone())
                {
                    continue;
                }
                out.push(GalleryItem {
                    title: if title.is_empty() {
                        format!("{} {}", kind.label(), out.len() + 1)
                    } else {
                        title.replace(['+', '＋'], "").trim().into()
                    },
                    image_url,
                    unit: String::new(),
                    price_text: String::new(),
                    kind: String::new(),
                });
            }
        }
    }
    out
}
fn image_extension(raw: &str) -> bool {
    let s = raw.split('?').next().unwrap_or(raw).to_ascii_lowercase();
    [".jpg", ".jpeg", ".png", ".gif", ".webp"]
        .iter()
        .any(|ext| s.ends_with(ext))
}
use std::collections::HashMap;
