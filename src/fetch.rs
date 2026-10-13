use crate::{model::*, parser::*, storage};
use percent_encoding::percent_decode_str;
use regex::Regex;
use std::{
    ffi::c_void,
    path::Path,
    ptr::{null, null_mut},
    sync::LazyLock,
    thread,
    time::{Duration, Instant},
};
use url::Url;
use windows_sys::Win32::{Foundation::GetLastError, Networking::WinHttp::*};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Handle(*mut c_void);
impl Handle {
    fn new(value: *mut c_void) -> Result<Self, String> {
        if value.is_null() {
            Err(format!("網絡服務錯誤 {}", unsafe { GetLastError() }))
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            WinHttpCloseHandle(self.0);
        }
    }
}
#[derive(Clone)]
struct Cookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    host_only: bool,
    expires: Option<Instant>,
}
pub struct Session {
    handle: Handle,
    cookies: Vec<Cookie>,
    requests: usize,
    last_request: Option<Instant>,
    deadline: Instant,
}
pub struct Response {
    pub status: u32,
    pub body: Vec<u8>,
    pub location: Option<String>,
}
impl Response {
    fn text(&self) -> Result<&str, String> {
        std::str::from_utf8(&self.body).map_err(|_| "來源文字編碼無法辨識".into())
    }
}
impl Session {
    pub fn new(seconds: u64) -> Result<Self, String> {
        unsafe {
            let agent = wide("gymL/1.0");
            let handle = Handle::new(WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            ))?;
            if WinHttpSetTimeouts(handle.0, 10000, 10000, 15000, 15000) == 0 {
                return Err("無法設定網絡逾時".into());
            }
            Ok(Self {
                handle,
                cookies: vec![],
                requests: 0,
                last_request: None,
                deadline: Instant::now() + Duration::from_secs(seconds),
            })
        }
    }
    fn cookies_for(&self, url: &Url) -> String {
        let host = url.host_str().unwrap_or("");
        self.cookies
            .iter()
            .filter(|c| {
                (host == c.domain || !c.host_only && host.ends_with(&format!(".{}", c.domain)))
                    && (url.path() == c.path
                        || url
                            .path()
                            .starts_with(&format!("{}/", c.path.trim_end_matches('/'))))
                    && c.expires.is_none_or(|e| Instant::now() < e)
            })
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }
    fn accept_cookie(&mut self, url: &Url, raw: &str) {
        let mut parts = raw.split(';');
        let Some((name, value)) = parts.next().and_then(|s| s.split_once('=')) else {
            return;
        };
        let host = url.host_str().unwrap_or("");
        let mut cookie = Cookie {
            name: name.trim().into(),
            value: value.trim().into(),
            domain: host.into(),
            path: url
                .path()
                .rsplit_once('/')
                .map(|(s, _)| if s.is_empty() { "/" } else { s })
                .unwrap_or("/")
                .into(),
            host_only: true,
            expires: None,
        };
        if cookie.name.is_empty() || cookie.name.len() > 100 || cookie.value.len() > 4096 {
            return;
        }
        for a in parts {
            let (k, v) = a.trim().split_once('=').unwrap_or((a.trim(), ""));
            match k.to_ascii_lowercase().as_str() {
                "domain" => {
                    cookie.domain = v.trim_start_matches('.').to_ascii_lowercase();
                    cookie.host_only = false;
                }
                "path" if v.starts_with('/') => cookie.path = v.into(),
                "max-age" => {
                    if let Ok(s) = v.parse::<i64>() {
                        cookie.expires =
                            Some(Instant::now() + Duration::from_secs(s.max(0) as u64));
                    }
                }
                _ => {}
            }
        }
        if !(host == cookie.domain || host.ends_with(&format!(".{}", cookie.domain)))
            || ![
                "mp.hkjebn.com",
                "www.hkjebn.com",
                "hkjebn.com",
                "hkjebn.queue-it.net",
                "queue-it.net",
            ]
            .contains(&cookie.domain.as_str())
        {
            return;
        }
        self.cookies.retain(|c| {
            !(c.name == cookie.name && c.domain == cookie.domain && c.path == cookie.path)
        });
        if cookie.expires.is_none_or(|e| e > Instant::now()) {
            self.cookies.push(cookie);
        }
    }
    fn has_php_session(&self) -> bool {
        self.cookies.iter().any(|c| {
            c.name == "PHPSESSID"
                && !c.value.is_empty()
                && c.domain == "mp.hkjebn.com"
                && c.expires.is_none_or(|e| Instant::now() < e)
        })
    }
    fn raw(
        &mut self,
        url: &Url,
        post: Option<&str>,
        referer: Option<&str>,
        limit: usize,
    ) -> Result<Response, String> {
        if self.requests >= 88 || Instant::now() > self.deadline {
            return Err("本次擷取達到請求或時間上限".into());
        }
        if let Some(last) = self.last_request {
            let elapsed = last.elapsed();
            if elapsed < Duration::from_millis(700) {
                thread::sleep(Duration::from_millis(700) - elapsed);
            }
        }
        self.last_request = Some(Instant::now());
        self.requests += 1;
        unsafe {
            let host = wide(url.host_str().ok_or("來源網址不正確")?);
            let connect = Handle::new(WinHttpConnect(self.handle.0, host.as_ptr(), 443, 0))?;
            let method = wide(if post.is_some() { "POST" } else { "GET" });
            let path = wide(&format!(
                "{}{}",
                url.path(),
                url.query().map(|q| format!("?{q}")).unwrap_or_default()
            ));
            let request = Handle::new(WinHttpOpenRequest(
                connect.0,
                method.as_ptr(),
                path.as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            ))?;
            let disabled = WINHTTP_DISABLE_REDIRECTS | WINHTTP_DISABLE_COOKIES;
            if WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_DISABLE_FEATURE,
                &disabled as *const _ as _,
                4,
            ) == 0
            {
                return Err("無法設定來源會話".into());
            }
            let mut headers = String::from("Accept: */*\r\nCache-Control: no-cache\r\n");
            let cookies = self.cookies_for(url);
            if !cookies.is_empty() {
                headers.push_str(&format!("Cookie: {cookies}\r\n"));
            }
            if post.is_some() {
                headers.push_str("Content-Type: application/x-www-form-urlencoded\r\n");
            }
            if let Some(r) = referer {
                headers.push_str(&format!("Referer: {r}\r\n"));
            }
            let headers = wide(&headers);
            let bytes = post.unwrap_or("").as_bytes();
            if WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                if bytes.is_empty() {
                    null()
                } else {
                    bytes.as_ptr() as _
                },
                bytes.len() as u32,
                bytes.len() as u32,
                0,
            ) == 0
                || WinHttpReceiveResponse(request.0, null_mut()) == 0
            {
                return Err(format!("來源連線失敗或逾時 ({})", GetLastError()));
            }
            let mut status = 0u32;
            let mut size = 4u32;
            if WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                &mut status as *mut _ as _,
                &mut size,
                null_mut(),
            ) == 0
            {
                return Err("來源缺少回應狀態".into());
            }
            let location = query_string(request.0, WINHTTP_QUERY_LOCATION, null_mut());
            let mut index = 0;
            while let Some(raw) = query_string(request.0, WINHTTP_QUERY_SET_COOKIE, &mut index) {
                self.accept_cookie(url, &raw);
                if self.cookies.len() > 64 {
                    return Err("來源會話資料異常".into());
                }
            }
            let mut body = Vec::with_capacity(16 * 1024);
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let mut read = 0;
                if WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr() as _,
                    buffer.len() as u32,
                    &mut read,
                ) == 0
                {
                    return Err("來源回應未完整讀取".into());
                }
                if read == 0 {
                    break;
                }
                if body.len() + read as usize > limit {
                    return Err("來源回應超過大小上限".into());
                }
                body.extend_from_slice(&buffer[..read as usize]);
                if Instant::now() > self.deadline {
                    return Err("來源讀取逾時".into());
                }
            }
            Ok(Response {
                status,
                body,
                location,
            })
        }
    }
    pub fn read(
        &mut self,
        input: &str,
        post: Option<&str>,
        referer: Option<&str>,
        limit: usize,
    ) -> Result<Response, String> {
        let original = Url::parse(input).map_err(|_| "來源網址不正確")?;
        let mut url = original.clone();
        for _ in 0..8 {
            if !allowed_redirect(&url, &original) {
                return Err("來源跳轉至不允許的頁面".into());
            }
            let response = self.raw(&url, post, referer, limit)?;
            let mut location = if [301, 302, 303, 307, 308].contains(&response.status) {
                response.location.clone()
            } else {
                None
            };
            if url.host_str() == Some("hkjebn.queue-it.net") && response.status == 200 {
                let html = response.text()?;
                static JUMP: LazyLock<Regex> = LazyLock::new(|| {
                    Regex::new(
                        r#"document\.location\.href\s*=\s*decodeURIComponent\(['"]([^'"]+)['"]\)"#,
                    )
                    .unwrap()
                });
                if html.contains("navigator.cookieEnabled") && html.contains("cookietest") {
                    location = JUMP
                        .captures(html)
                        .map(|c| {
                            percent_decode_str(&c[1])
                                .decode_utf8()
                                .map(|s| s.into_owned())
                        })
                        .transpose()
                        .map_err(|_| "來源跳轉格式不正確")?;
                }
                if location.is_none() {
                    return Err("來源正在排隊或要求互動驗證；請稍後重試".into());
                }
            }
            if let Some(target) = location {
                if post.is_some() {
                    return Err("分頁期間來源要求重新建立會話".into());
                }
                url = url.join(&target).map_err(|_| "來源跳轉格式不正確")?;
                continue;
            }
            return Ok(response);
        }
        Err("來源跳轉超過安全上限".into())
    }
}
unsafe fn query_string(request: *mut c_void, query: u32, index: *mut u32) -> Option<String> {
    unsafe {
        let mut size = 0;
        let mut probe = if index.is_null() { 0 } else { *index };
        WinHttpQueryHeaders(
            request,
            query,
            null(),
            null_mut(),
            &mut size,
            if index.is_null() {
                null_mut()
            } else {
                &mut probe
            },
        );
        if size == 0 || size > 65536 {
            return None;
        }
        let mut buf = vec![0u16; size as usize / 2 + 1];
        if WinHttpQueryHeaders(
            request,
            query,
            null(),
            buf.as_mut_ptr() as _,
            &mut size,
            index,
        ) == 0
        {
            return None;
        }
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..end]))
    }
}
pub fn allowed_redirect(url: &Url, original: &Url) -> bool {
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return false;
    }
    if url.host_str() == Some("hkjebn.queue-it.net") {
        return url.path() == "/";
    }
    if url.origin() != original.origin() {
        return false;
    }
    if original.path().starts_with("/tw/hk/promotion/") {
        return url.path().starts_with("/tw/hk/promotion/");
    }
    url.path() == original.path()
        && ["cat", "subcat", "mode"].iter().all(|key| {
            url.query_pairs()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.into_owned())
                == original
                    .query_pairs()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.into_owned())
        })
}

pub fn collect_pages(
    mut read: impl FnMut(usize) -> Result<Vec<Product>, String>,
    limit: usize,
) -> Result<Vec<Product>, String> {
    let mut products = vec![];
    let mut previous = std::collections::HashSet::new();
    for page in 1..=limit {
        let rows = read(page)?;
        if rows.is_empty() {
            return if page == 1 {
                Err("來源第一頁空白；未確認分類完整性".into())
            } else {
                Ok(deduplicate(products))
            };
        }
        let ids: Vec<_> = rows.iter().map(|p| p.id.clone()).collect();
        if ids.iter().all(|id| previous.contains(id)) {
            return Err("來源重複分頁；未確認分類完整性".into());
        }
        previous.extend(ids);
        products.extend(rows);
        if products.len() > 10000 {
            return Err("來源商品超過安全上限".into());
        }
    }
    Err("來源未回傳明確末頁；未取代完整快取".into())
}
pub fn refresh_catalog(mut progress: impl FnMut(String)) -> Result<Catalog, String> {
    let mut products = vec![];
    for category in [Category::Seafood, Category::Meat] {
        progress(format!("{} · 建立匿名來源會話…", category.label()));
        let mut session = Session::new(150)?;
        let entry = format!("{}&mode=detail&filter_type=", category.source());
        let response = session.read(&entry, None, None, 2_000_000)?;
        let html = response.text()?;
        check_html(response.status, html, true)?;
        let subcat = if category == Category::Seafood {
            "1409"
        } else {
            "1620"
        };
        let confirmation = Regex::new(&format!(r#"var\s+subcat\s*=\s*['"]{subcat}['"]"#)).unwrap();
        if !confirmation.is_match(html) || !session.has_php_session() {
            return Err("來源未確認分類或建立分頁會話".into());
        }
        let rows = collect_pages(
            |page| {
                progress(format!("{} · 第 {page} 頁…", category.label()));
                let body = format!("mode=detail&page={page}&filter_type=");
                let response = session.read(
                    "https://mp.hkjebn.com/product-data.php",
                    Some(&body),
                    Some(&entry),
                    2_000_000,
                )?;
                let html = response.text()?;
                if response.status == 200 && html.trim().is_empty() {
                    return Ok(vec![]);
                }
                check_html(response.status, html, true)?;
                let rows = parse_products(html, category)?;
                if rows.is_empty() {
                    return Err("來源非空分頁缺少商品；保留完整快取".into());
                }
                Ok(rows)
            },
            20,
        )?;
        products.extend(rows);
    }
    let catalog = Catalog {
        version: 1,
        fetched_at: now(),
        products: deduplicate(products),
    };
    catalog.validate()?;
    Ok(catalog)
}
pub fn refresh_gallery(
    kind: GalleryKind,
    mut progress: impl FnMut(String),
) -> Result<Gallery, String> {
    progress(format!("{} · 讀取來源索引…", kind.label()));
    let mut session = Session::new(120)?;
    let response = session.read(kind.source(), None, None, 2_000_000)?;
    check_html(response.status, response.text()?, false)?;
    let html = response.text()?;
    let mut items = parse_gallery(html, kind);
    let links = gallery_links(html, kind);
    if links.len() > 20 {
        return Err("來源圖庫索引超過安全上限".into());
    }
    for (index, link) in links.iter().enumerate() {
        progress(format!("{} · {}/{}…", kind.label(), index + 1, links.len()));
        let response = session.read(&link.url, None, None, 2_000_000)?;
        check_html(response.status, response.text()?, false)?;
        let html = if kind == GalleryKind::Coupon {
            format!("<div id='coupon_div'>{}</div>", response.text()?)
        } else {
            response.text()?.to_string()
        };
        let extracted = parse_gallery(&html, kind);
        if extracted.is_empty() {
            return Err("部分圖庫項目缺少圖片；保留原有快取".into());
        }
        for mut item in extracted {
            if !link.title.is_empty() {
                item.title = link.title.replace(['+', '＋'], "").trim().into();
            }
            item.unit = link.unit.clone();
            item.price_text = link.price_text.clone();
            item.kind = link.kind.clone();
            if let Some(old) = items.iter_mut().find(|old| old.image_url == item.image_url) {
                *old = item;
            } else {
                items.push(item);
            }
        }
    }
    let gallery = Gallery {
        fetched_at: now(),
        items,
    };
    gallery.validate()?;
    Ok(gallery)
}
pub struct ImageBytes {
    pub bytes: Vec<u8>,
    pub downloaded: bool,
}
pub fn fetch_image(dir: &Path, url: &str) -> Result<ImageBytes, String> {
    if !safe_retail_url(url) {
        return Err("圖片來源不允許".into());
    }
    if let Some(bytes) = storage::cached_image(dir, url) {
        return Ok(ImageBytes {
            bytes,
            downloaded: false,
        });
    }
    let mut session = Session::new(30)?;
    let response = session.read(url, None, None, 16_000_000)?;
    if response.status != 200 {
        return Err(format!("圖片來源 HTTP {}", response.status));
    }
    Ok(ImageBytes {
        bytes: response.body,
        downloaded: true,
    })
}
