use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

pub const SEAFOOD: &str = "https://mp.hkjebn.com/product.php?cat=1408&subcat=1409";
pub const MEAT: &str = "https://mp.hkjebn.com/product.php?cat=1408&subcat=1620";
pub const COUPON: &str = "https://mp.hkjebn.com/";
pub const PROMOTION: &str = "https://www.hkjebn.com/tw/hk/promotion/";
pub const CATEGORY_LABELS: [&str; 8] = ["全部", "海產", "肉類", "魚", "雞", "牛", "豬", "蝦"];
pub const SORT_LABELS: [&str; 5] = ["預設", "分組", "價格由低至高", "價格由高至低", "會員折扣"];
pub const EXPENSIVE: [&str; 3] = ["海參", "青邊鮑", "螺頭"];
pub const EXCLUDED: [&str; 4] = ["鱷魚", "中國", "魚尾", "自用級"];
pub const GALLERY_HIDDEN: [&str; 8] = [
    "燕窩",
    "冬蟲夏草",
    "蟲草",
    "海參",
    "乾鮑",
    "遼參",
    "刺參",
    "中國",
];

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Seafood,
    Meat,
}
impl Category {
    pub fn source(self) -> &'static str {
        match self {
            Self::Seafood => SEAFOOD,
            Self::Meat => MEAT,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Seafood => "海產",
            Self::Meat => "肉類",
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Promotion {
    Like,
    Special,
    Regular,
    Unknown,
}
impl Promotion {
    pub fn label(self) -> &'static str {
        match self {
            Self::Like => "至Like價",
            Self::Special => "特價",
            Self::Regular => "正價",
            Self::Unknown => "未確認",
        }
    }
    fn rank(self) -> u8 {
        match self {
            Self::Like => 0,
            Self::Special => 1,
            Self::Regular => 2,
            Self::Unknown => 3,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Prices {
    pub original: Option<f64>,
    pub selling: Option<f64>,
    pub member: Option<f64>,
}
impl Prices {
    pub fn valid(&self) -> bool {
        [self.original, self.selling, self.member]
            .into_iter()
            .flatten()
            .all(|v| v.is_finite() && (0.0..=10000000.0).contains(&v))
    }
    pub fn empty(&self) -> bool {
        self.original.is_none() && self.selling.is_none() && self.member.is_none()
    }
    fn merge(&mut self, next: &Self) {
        if next.original.is_some() {
            self.original = next.original;
        }
        if next.selling.is_some() {
            self.selling = next.selling;
        }
        if next.member.is_some() {
            self.member = next.member;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PriceBatch {
    pub raw_date: String,
    pub date_iso: Option<String>,
    pub unchanged: bool,
    pub prices: Prices,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub categories: Vec<Category>,
    pub unit: String,
    pub image: Option<String>,
    pub url: String,
    pub promotion: Promotion,
    pub promotion_labels: Vec<String>,
    pub prices: Prices,
    pub price_batches: Vec<PriceBatch>,
}
impl Product {
    pub fn changed_batches(&self) -> Vec<&PriceBatch> {
        let mut previous = self.prices.clone();
        self.price_batches
            .iter()
            .filter(|b| {
                if b.unchanged {
                    return false;
                }
                let changed = (b.prices.original.is_some()
                    && b.prices.original != previous.original)
                    || (b.prices.selling.is_some() && b.prices.selling != previous.selling)
                    || (b.prices.member.is_some() && b.prices.member != previous.member);
                previous.merge(&b.prices);
                changed
            })
            .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub version: u32,
    pub fetched_at: u64,
    pub products: Vec<Product>,
}
impl Catalog {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.fetched_at == 0
            || self.products.is_empty()
            || self.products.len() > 10000
        {
            return Err("商品快取格式不正確".into());
        }
        let mut ids = HashSet::new();
        for p in &self.products {
            let u = Url::parse(&p.url).map_err(|_| "商品連結格式不正確")?;
            if p.id.is_empty()
                || !p.id.bytes().all(|c| c.is_ascii_digit())
                || !ids.insert(&p.id)
                || p.name.trim().is_empty()
                || p.name.len() > 2000
                || p.unit.len() > 2000
                || !safe_retail_url(&p.url)
                || u.path() != "/product-detail.php"
                || !u
                    .query_pairs()
                    .any(|(k, v)| k == "product_code" && v == p.id)
                || p.image.as_ref().is_some_and(|s| !safe_retail_url(s))
                || p.categories.is_empty()
                || p.categories.len() > 2
                || (p.categories.len() == 2 && p.categories[0] == p.categories[1])
                || p.prices.empty()
                || !p.prices.valid()
                || p.price_batches.len() > 100
                || p.promotion_labels.len() > 20
                || p.price_batches.iter().any(|b| {
                    !b.prices.valid()
                        || b.raw_date.len() > 200
                        || b.date_iso != crate::parser::effective_date(&b.raw_date)
                })
            {
                return Err("商品資料未通過驗證".into());
            }
        }
        for c in [Category::Seafood, Category::Meat] {
            if !self.products.iter().any(|p| p.categories.contains(&c)) {
                return Err("商品分類不完整".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Default,
    Group,
    PriceAsc,
    PriceDesc,
    Discount,
}
impl SortMode {
    pub fn index(self) -> usize {
        match self {
            Self::Default => 0,
            Self::Group => 1,
            Self::PriceAsc => 2,
            Self::PriceDesc => 3,
            Self::Discount => 4,
        }
    }
    pub fn from_index(i: usize) -> Self {
        match i {
            1 => Self::Group,
            2 => Self::PriceAsc,
            3 => Self::PriceDesc,
            4 => Self::Discount,
            _ => Self::Default,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub like: bool,
    pub special: bool,
    pub regular: bool,
    pub expensive: bool,
    pub excluded: bool,
    pub latest: bool,
    pub category: usize,
    pub sort: SortMode,
    pub search: String,
    pub gallery_keywords: Vec<String>,
    pub gallery_show_all: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            like: true,
            special: true,
            regular: true,
            expensive: false,
            excluded: false,
            latest: false,
            category: 0,
            sort: SortMode::Default,
            search: String::new(),
            gallery_keywords: GALLERY_HIDDEN.iter().map(|s| s.to_string()).collect(),
            gallery_show_all: false,
        }
    }
}

pub fn safe_retail_url(raw: &str) -> bool {
    Url::parse(raw).is_ok_and(|u| {
        u.scheme() == "https"
            && u.username().is_empty()
            && u.password().is_none()
            && u.port().is_none()
            && u.host_str()
                .is_some_and(|h| h == "hkjebn.com" || h == "www.hkjebn.com" || h == "mp.hkjebn.com")
    })
}
pub fn retail_url(raw: &str, base: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return None;
    }
    let mut u = Url::parse(base).ok()?.join(raw.trim()).ok()?;
    if u.scheme() == "http" {
        u.set_scheme("https").ok()?;
    }
    u.set_fragment(None);
    safe_retail_url(u.as_str()).then(|| u.into())
}

pub fn deduplicate(products: Vec<Product>) -> Vec<Product> {
    let mut out: Vec<Product> = Vec::with_capacity(products.len());
    let mut ids = HashMap::new();
    for p in products {
        if let Some(&i) = ids.get(&p.id) {
            let old: &mut Product = &mut out[i];
            for c in p.categories {
                if !old.categories.contains(&c) {
                    old.categories.push(c);
                }
            }
        } else {
            ids.insert(p.id.clone(), out.len());
            out.push(p);
        }
    }
    out
}
pub fn discount(p: &Prices) -> Option<f64> {
    let base = p.original.or(p.selling)?;
    let member = p.member?;
    (base > 0.0).then(|| ((1.0 - member / base) * 1000.0).round() / 10.0)
}
pub fn matches_category(p: &Product, category: usize) -> bool {
    match category {
        0 => true,
        1 => p.categories.contains(&Category::Seafood),
        2 => p.categories.contains(&Category::Meat),
        3..=7 => p
            .name
            .contains(["魚", "雞", "牛", "豬", "蝦"][category - 3]),
        _ => false,
    }
}
fn group(p: &Product) -> usize {
    ["魚", "雞", "牛", "豬", "蝦"]
        .iter()
        .position(|s| p.name.contains(s))
        .unwrap_or(5)
}

pub fn latest_batch(products: &[Product]) -> Option<(bool, String)> {
    let batches: Vec<_> = products.iter().flat_map(|p| p.changed_batches()).collect();
    if batches.is_empty() {
        return None;
    }
    if batches.iter().all(|b| b.date_iso.is_none()) {
        let raw = &batches[0].raw_date;
        if !raw.is_empty() && batches.iter().all(|b| &b.raw_date == raw) {
            return Some((false, raw.clone()));
        }
        return None;
    }
    if batches.iter().any(|b| b.date_iso.is_none()) {
        return None;
    }
    batches
        .iter()
        .filter_map(|b| b.date_iso.as_ref())
        .max()
        .map(|s| (true, s.clone()))
}

pub fn select_products(products: &[Product], prefs: &Preferences) -> Vec<usize> {
    let query = prefs.search.trim().to_lowercase();
    let latest = if prefs.latest {
        latest_batch(products)
    } else {
        None
    };
    let mut indices: Vec<_> = products
        .iter()
        .enumerate()
        .filter(|(_, p)| {
            let expensive = EXPENSIVE.iter().any(|s| p.name.contains(s));
            let excluded = EXCLUDED
                .iter()
                .any(|s| p.name.contains(s) || (*s == "自用級" && p.unit.contains(s)));
            let promotion = match p.promotion {
                Promotion::Like => prefs.like,
                Promotion::Special => prefs.special,
                Promotion::Regular => prefs.regular,
                Promotion::Unknown => prefs.like && prefs.special && prefs.regular,
            };
            matches_category(p, prefs.category)
                && (query.is_empty() || p.name.to_lowercase().contains(&query))
                && (prefs.expensive || !expensive)
                && (prefs.excluded || !excluded)
                && (prefs.latest
                    || prefs.expensive && expensive
                    || prefs.excluded && excluded
                    || promotion)
                && (!prefs.latest
                    || latest.as_ref().is_some_and(|(full, date)| {
                        p.changed_batches().iter().any(|b| {
                            if *full {
                                b.date_iso.as_ref() == Some(date)
                            } else {
                                &b.raw_date == date
                            }
                        })
                    }))
        })
        .map(|(i, _)| i)
        .collect();
    indices.sort_by(|&a, &b| {
        let a = &products[a];
        let b = &products[b];
        let fallback = group(a)
            .cmp(&group(b))
            .then(a.promotion.rank().cmp(&b.promotion.rank()));
        if prefs.sort == SortMode::Default {
            let rank = |p: &Product| match p.promotion {
                Promotion::Like => 0,
                Promotion::Special => 1,
                _ if !p.changed_batches().is_empty() => 2,
                Promotion::Regular => 3,
                _ => 4,
            };
            return rank(a).cmp(&rank(b));
        }
        if prefs.sort == SortMode::Group {
            return fallback;
        }
        let value = |p: &Product| {
            if prefs.sort == SortMode::Discount {
                discount(&p.prices)
            } else {
                p.prices.member.or(p.prices.selling)
            }
        };
        match (value(a), value(b)) {
            (Some(a), Some(b)) => {
                let ord = a.partial_cmp(&b).unwrap_or(Ordering::Equal);
                (if prefs.sort == SortMode::PriceAsc {
                    ord
                } else {
                    ord.reverse()
                })
                .then(fallback)
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            _ => fallback,
        }
    });
    indices
}

pub fn money(value: Option<f64>) -> String {
    value
        .map(|v| format!("${v:.2}"))
        .unwrap_or_else(|| "—".into())
}

pub fn captured_time(timestamp: u64) -> String {
    let local = timestamp.saturating_add(9 * 3600);
    let days = (local / 86400) as i64 + 719468;
    let era = days / 146097;
    let day_of_era = days - era * 146097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}/{month:02}/{day:02} {:02}:{:02}",
        (local % 86400) / 3600,
        (local % 3600) / 60
    )
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GalleryKind {
    Coupon,
    Promotion,
}
impl GalleryKind {
    pub fn source(self) -> &'static str {
        match self {
            Self::Coupon => COUPON,
            Self::Promotion => PROMOTION,
        }
    }
    pub fn file(self) -> &'static str {
        match self {
            Self::Coupon => "coupons.json",
            Self::Promotion => "promotions.json",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Coupon => "優惠",
            Self::Promotion => "推廣",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GalleryItem {
    pub title: String,
    pub image_url: String,
    pub unit: String,
    pub price_text: String,
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gallery {
    pub fetched_at: u64,
    pub items: Vec<GalleryItem>,
}
impl Gallery {
    pub fn validate(&self) -> Result<(), String> {
        if self.fetched_at == 0
            || self.items.is_empty()
            || self.items.len() > 1000
            || self.items.iter().any(|i| {
                !safe_retail_url(&i.image_url)
                    || i.title.len() > 2000
                    || i.unit.len() > 2000
                    || i.price_text.len() > 2000
            })
        {
            Err("圖庫快取格式不正確".into())
        } else {
            Ok(())
        }
    }
}
