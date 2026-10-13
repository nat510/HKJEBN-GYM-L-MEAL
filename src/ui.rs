use gyml::{fetch, model::*, storage};
use std::{
    cell::RefCell,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::*, SystemServices::*},
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

const EVENT: u32 = WM_APP + 1;
const ID_SEARCH: u16 = 10;
const ID_CATEGORY: u16 = 11;
const ID_SORT: u16 = 12;
const ID_REFRESH: u16 = 13;
const ID_RESET: u16 = 14;
const ID_LIST: u16 = 15;
const ID_OPEN: u16 = 16;
const ID_KEYWORDS: u16 = 17;
const ID_ALL: u16 = 18;
const ID_PRODUCT: u16 = 20;
const ID_COUPON: u16 = 21;
const ID_PROMOTION: u16 = 22;
const FILTER_START: u16 = 30;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Product,
    Coupon,
    Promotion,
}
impl Mode {
    fn gallery(self) -> Option<GalleryKind> {
        match self {
            Self::Coupon => Some(GalleryKind::Coupon),
            Self::Promotion => Some(GalleryKind::Promotion),
            _ => None,
        }
    }
}
enum Event {
    Progress(String),
    Catalog(Result<Catalog, String>),
    Gallery(GalleryKind, Result<Gallery, String>),
    Done(storage::CleanupReport),
    Image(Result<crate::viewer::Decoded, String>),
    Warning(String),
}
#[derive(Clone, Copy)]
struct Controls {
    title: HWND,
    search: HWND,
    category: HWND,
    sort: HWND,
    refresh: HWND,
    reset: HWND,
    modes: [HWND; 3],
    filters: [HWND; 6],
    all: HWND,
    keywords: HWND,
    list: HWND,
    details: HWND,
    open: HWND,
    status: HWND,
    count: HWND,
}
impl Controls {
    fn all(self) -> Vec<HWND> {
        [
            vec![
                self.title,
                self.search,
                self.category,
                self.sort,
                self.refresh,
                self.reset,
                self.all,
                self.keywords,
                self.list,
                self.details,
                self.open,
                self.status,
                self.count,
            ],
            self.modes.to_vec(),
            self.filters.to_vec(),
        ]
        .concat()
    }
}
struct App {
    hwnd: HWND,
    controls: Controls,
    font: HFONT,
    dir: PathBuf,
    prefs: Preferences,
    catalog: Option<Catalog>,
    galleries: [Option<Gallery>; 2],
    mode: Mode,
    visible: Vec<usize>,
    rx: Receiver<Event>,
    tx: Sender<Event>,
    refreshing: bool,
    image_busy: bool,
    errors: Vec<String>,
    status: String,
    viewer_index: Option<usize>,
    prefs_dirty: bool,
}
thread_local! {static APP:RefCell<Option<App>>=const{RefCell::new(None)};}
fn read<R>(f: impl FnOnce(&App) -> R) -> Option<R> {
    APP.with(|cell| cell.borrow().as_ref().map(f))
}
fn edit<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.borrow_mut().as_mut().map(f))
}
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn set_text(hwnd: HWND, s: &str) {
    unsafe {
        SetWindowTextW(hwnd, wide(s).as_ptr());
    }
}
fn text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd).clamp(0, 20000) as usize;
        let mut buf = vec![0u16; len + 1];
        let count = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..count.max(0) as usize])
    }
}
fn checked(hwnd: HWND) -> bool {
    unsafe { SendMessageW(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as isize }
}
fn combo(hwnd: HWND) -> usize {
    unsafe { SendMessageW(hwnd, CB_GETCURSEL, 0, 0).max(0) as usize }
}
fn check(hwnd: HWND, value: bool) {
    unsafe {
        SendMessageW(
            hwnd,
            BM_SETCHECK,
            if value { BST_CHECKED } else { BST_UNCHECKED } as usize,
            0,
        );
    }
}

pub fn run() -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let icc = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&icc);
        let instance = GetModuleHandleW(null());
        let class = wide("gymL.Native");
        let icon = LoadIconW(instance, 1usize as _);
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hIcon: icon,
            hIconSm: icon,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err("無法建立原生視窗".into());
        }
        crate::viewer::register(instance)?;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class.as_ptr(),
            wide("gymL · 蛋白 · 食多啲").as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (1200 * GetDpiForSystem().max(96) / 96) as i32,
            (780 * GetDpiForSystem().max(96) / 96) as i32,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            return Err("無法開啟 gymL".into());
        }
        initialize(hwnd, storage::data_dir()?)?;
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        start_refresh();
        let mut msg = MSG::default();
        loop {
            let result = GetMessageW(&mut msg, null_mut(), 0, 0);
            if result <= 0 {
                break;
            }
            if msg.message == WM_KEYDOWN {
                let view = crate::viewer::handle();
                let editor = crate::viewer::editor_handle();
                let root = GetAncestor(msg.hwnd, GA_ROOT);
                if msg.wParam == VK_RETURN as usize && read(|a| a.controls.list) == Some(msg.hwnd) {
                    open_image();
                    continue;
                }
                if !view.is_null()
                    && root == view
                    && [VK_LEFT as usize, VK_RIGHT as usize].contains(&msg.wParam)
                {
                    move_image(if msg.wParam == VK_LEFT as usize {
                        -1
                    } else {
                        1
                    });
                    continue;
                }
                if msg.wParam == VK_ESCAPE as usize && !editor.is_null() && root == editor {
                    crate::viewer::close_editor();
                    continue;
                }
                if msg.wParam == VK_F5 as usize {
                    start_refresh();
                    continue;
                }
                if msg.wParam == b'F' as usize && GetKeyState(VK_CONTROL as i32) < 0 {
                    if let Some(search) = read(|a| a.controls.search) {
                        SetFocus(search);
                        SendMessageW(search, EM_SETSEL, 0, -1);
                    }
                    continue;
                }
                if msg.wParam == VK_ESCAPE as usize && !crate::viewer::handle().is_null() {
                    crate::viewer::close();
                    continue;
                }
            }
            let view = crate::viewer::handle();
            let editor = crate::viewer::editor_handle();
            if !editor.is_null() && IsDialogMessageW(editor, &msg) != 0 {
                continue;
            }
            if !view.is_null() && IsDialogMessageW(view, &msg) != 0 {
                continue;
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }
}

fn control(parent: HWND, class: &str, label: &str, id: u16, style: u32, ex: u32) -> HWND {
    unsafe {
        CreateWindowExW(
            ex,
            wide(class).as_ptr(),
            wide(label).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            100,
            28,
            parent,
            id as usize as _,
            GetModuleHandleW(null()),
            null(),
        )
    }
}
fn button(parent: HWND, label: &str, id: u16) -> HWND {
    control(
        parent,
        "BUTTON",
        label,
        id,
        WS_TABSTOP | BS_PUSHBUTTON as u32,
        0,
    )
}
fn initialize(hwnd: HWND, dir: PathBuf) -> Result<(), String> {
    unsafe {
        let prefs = storage::load_preferences(&dir);
        let catalog = storage::load_catalog(&dir).ok();
        let galleries = [
            storage::load_gallery(&dir, GalleryKind::Coupon).ok(),
            storage::load_gallery(&dir, GalleryKind::Promotion).ok(),
        ];
        let filters = std::array::from_fn(|i| {
            control(
                hwnd,
                "BUTTON",
                ["至Like價", "特價", "正價", "貴", "渣", "異動"][i],
                FILTER_START + i as u16,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                0,
            )
        });
        let controls = Controls {
            title: control(hwnd, "STATIC", "gymL   蛋白 · 食多啲", 0, SS_LEFT, 0),
            search: control(
                hwnd,
                "EDIT",
                &prefs.search,
                ID_SEARCH,
                WS_TABSTOP | ES_AUTOHSCROLL as u32,
                WS_EX_CLIENTEDGE,
            ),
            category: control(
                hwnd,
                "COMBOBOX",
                "",
                ID_CATEGORY,
                WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                0,
            ),
            sort: control(
                hwnd,
                "COMBOBOX",
                "",
                ID_SORT,
                WS_TABSTOP | CBS_DROPDOWNLIST as u32 | WS_VSCROLL,
                0,
            ),
            refresh: button(hwnd, "更新 (F5)", ID_REFRESH),
            reset: button(hwnd, "重設", ID_RESET),
            modes: [
                button(hwnd, "商品", ID_PRODUCT),
                button(hwnd, "優惠", ID_COUPON),
                button(hwnd, "推廣", ID_PROMOTION),
            ],
            filters,
            all: control(
                hwnd,
                "BUTTON",
                "顯示全部",
                ID_ALL,
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
                0,
            ),
            keywords: button(hwnd, "編輯隱藏關鍵字", ID_KEYWORDS),
            list: control(
                hwnd,
                "SysListView32",
                "",
                ID_LIST,
                WS_TABSTOP | LVS_REPORT | LVS_OWNERDATA | LVS_SINGLESEL | LVS_SHOWSELALWAYS,
                WS_EX_CLIENTEDGE,
            ),
            details: control(
                hwnd,
                "EDIT",
                "選取商品查看價格詳情；按 Enter 或雙擊查看圖片。",
                0,
                ES_MULTILINE as u32 | ES_READONLY as u32 | WS_VSCROLL,
                WS_EX_CLIENTEDGE,
            ),
            open: button(hwnd, "查看圖片", ID_OPEN),
            status: control(hwnd, "STATIC", "每次開啟都重新擷取來源…", 0, SS_LEFT, 0),
            count: control(hwnd, "STATIC", "", 0, SS_RIGHT, 0),
        };
        if controls.all().iter().any(|h| h.is_null()) {
            return Err("原生控制項建立失敗".into());
        }
        for label in CATEGORY_LABELS {
            SendMessageW(
                controls.category,
                CB_ADDSTRING,
                0,
                wide(label).as_ptr() as isize,
            );
        }
        for label in SORT_LABELS {
            SendMessageW(
                controls.sort,
                CB_ADDSTRING,
                0,
                wide(label).as_ptr() as isize,
            );
        }
        SendMessageW(
            controls.search,
            EM_SETCUEBANNER,
            1,
            wide("搜尋完整商品名稱…").as_ptr() as isize,
        );
        SendMessageW(controls.search, EM_SETLIMITTEXT, 200, 0);
        SendMessageW(
            controls.list,
            LVM_SETEXTENDEDLISTVIEWSTYLE,
            0,
            (LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER | LVS_EX_GRIDLINES) as isize,
        );
        SendMessageW(controls.list, LVM_SETUNICODEFORMAT, 1, 0);
        let (tx, rx) = mpsc::channel();
        APP.with(|s| {
            *s.borrow_mut() = Some(App {
                hwnd,
                controls,
                font: null_mut(),
                dir,
                prefs,
                catalog,
                galleries,
                mode: Mode::Product,
                visible: vec![],
                rx,
                tx,
                refreshing: false,
                image_busy: false,
                errors: vec![],
                status: "快取資料 · 正在重新擷取來源".into(),
                viewer_index: None,
                prefs_dirty: false,
            })
        });
        set_font();
        sync_controls();
        columns();
        rebuild();
        layout();
        Ok(())
    }
}

fn set_font() {
    unsafe {
        let Some((hwnd, controls, old)) = read(|a| (a.hwnd, a.controls, a.font)) else {
            return;
        };
        let dpi = GetDpiForWindow(hwnd).max(96);
        let font = CreateFontW(
            -((16 * dpi / 96) as i32),
            0,
            0,
            0,
            FW_NORMAL as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            DEFAULT_PITCH as u32,
            wide("Segoe UI").as_ptr(),
        );
        edit(|a| a.font = font);
        for h in controls.all() {
            SendMessageW(h, WM_SETFONT, font as usize, 1);
        }
        crate::viewer::update_font(font);
        if !old.is_null() {
            DeleteObject(old);
        }
    }
}
fn sync_controls() {
    unsafe {
        let Some((c, p)) = read(|a| (a.controls, a.prefs.clone())) else {
            return;
        };
        SendMessageW(c.category, CB_SETCURSEL, p.category, 0);
        SendMessageW(c.sort, CB_SETCURSEL, p.sort.index(), 0);
        for (h, value) in c.filters.into_iter().zip([
            p.like,
            p.special,
            p.regular,
            p.expensive,
            p.excluded,
            p.latest,
        ]) {
            check(h, value);
        }
        check(c.all, p.gallery_show_all);
    }
}
fn columns() {
    unsafe {
        let Some((c, mode, dpi)) = read(|a| (a.controls, a.mode, GetDpiForWindow(a.hwnd).max(96)))
        else {
            return;
        };
        SendMessageW(c.list, WM_SETREDRAW, 0, 0);
        while SendMessageW(c.list, LVM_DELETECOLUMN, 0, 0) != 0 {}
        let labels = if mode == Mode::Product {
            [
                "商品名稱",
                "單位",
                "類別",
                "原價",
                "售價",
                "會員價",
                "折扣",
                "生效價目",
            ]
        } else {
            ["名稱", "單位", "價錢", "種類", "", "", "", ""]
        };
        let widths = if mode == Mode::Product {
            [340, 170, 85, 85, 85, 85, 70, 200]
        } else {
            [480, 160, 180, 150, 0, 0, 0, 0]
        };
        for (i, (&label, &width)) in labels.iter().zip(widths.iter()).enumerate() {
            let mut w = wide(label);
            let col = LVCOLUMNW {
                mask: LVCF_TEXT | LVCF_WIDTH | LVCF_FMT,
                fmt: if (3..=6).contains(&i) && mode == Mode::Product {
                    LVCFMT_RIGHT
                } else {
                    LVCFMT_LEFT
                },
                cx: width * dpi as i32 / 96,
                pszText: w.as_mut_ptr(),
                ..Default::default()
            };
            SendMessageW(c.list, LVM_INSERTCOLUMNW, i, &col as *const _ as isize);
        }
        SendMessageW(c.list, WM_SETREDRAW, 1, 0);
    }
}
fn gallery_index(kind: GalleryKind) -> usize {
    if kind == GalleryKind::Coupon { 0 } else { 1 }
}
fn rebuild() {
    let Some((controls, len, total, mode, status)) = edit(|a| {
        a.visible = if a.mode == Mode::Product {
            a.catalog
                .as_ref()
                .map(|c| select_products(&c.products, &a.prefs))
                .unwrap_or_default()
        } else {
            a.galleries[gallery_index(a.mode.gallery().unwrap())]
                .as_ref()
                .map(|g| {
                    g.items
                        .iter()
                        .enumerate()
                        .filter(|(_, i)| {
                            a.prefs.gallery_show_all
                                || !a.prefs.gallery_keywords.iter().any(|s| i.title.contains(s))
                        })
                        .filter(|(_, i)| {
                            a.prefs.search.trim().is_empty()
                                || i.title
                                    .to_lowercase()
                                    .contains(&a.prefs.search.trim().to_lowercase())
                        })
                        .map(|(i, _)| i)
                        .collect()
                })
                .unwrap_or_default()
        };
        let total = if a.mode == Mode::Product {
            a.catalog.as_ref().map_or(0, |c| c.products.len())
        } else {
            a.galleries[gallery_index(a.mode.gallery().unwrap())]
                .as_ref()
                .map_or(0, |g| g.items.len())
        };
        let captured = if a.mode == Mode::Product {
            a.catalog.as_ref().map(|c| c.fetched_at)
        } else {
            a.galleries[gallery_index(a.mode.gallery().unwrap())]
                .as_ref()
                .map(|g| g.fetched_at)
        };
        let status = if let Some(t) = captured {
            format!(
                "{}\r\n資料擷取：{} 日本時間 · 價格以樓上原站為準",
                a.status,
                captured_time(t)
            )
        } else {
            a.status.clone()
        };
        (a.controls, a.visible.len(), total, a.mode, status)
    }) else {
        return;
    };
    unsafe {
        SendMessageW(controls.list, LVM_SETITEMCOUNT, len, 0);
        InvalidateRect(controls.list, null(), 1);
    }
    set_text(
        controls.count,
        &format!(
            "{}   {len} / {total}",
            if mode == Mode::Product {
                "商品"
            } else {
                mode.gallery().unwrap().label()
            }
        ),
    );
    set_text(controls.status, &status);
    set_text(
        controls.details,
        if len == 0 {
            "目前沒有符合條件的項目。可調整篩選或按更新重新擷取。"
        } else {
            "選取項目查看詳情；按 Enter 或雙擊查看圖片。"
        },
    );
    unsafe {
        for h in [controls.category, controls.sort, controls.reset]
            .into_iter()
            .chain(controls.filters)
        {
            ShowWindow(
                h,
                if mode == Mode::Product {
                    SW_SHOW
                } else {
                    SW_HIDE
                },
            );
        }
        for h in [controls.all, controls.keywords] {
            ShowWindow(
                h,
                if mode == Mode::Product {
                    SW_HIDE
                } else {
                    SW_SHOW
                },
            );
        }
    }
}
fn layout() {
    unsafe {
        let Some((hwnd, c)) = read(|a| (a.hwnd, a.controls)) else {
            return;
        };
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect);
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        let scale = |n: i32| n * dpi / 96;
        let width = rect.right;
        let height = rect.bottom;
        let place = |h: HWND, x: i32, y: i32, w: i32, ht: i32| {
            MoveWindow(h, scale(x), scale(y), scale(w).max(1), scale(ht).max(1), 1);
        };
        let logical_width = width * 96 / dpi;
        let logical_height = height * 96 / dpi;
        place(c.title, 16, 14, 320, 30);
        for (i, h) in c.modes.into_iter().enumerate() {
            place(h, logical_width - 340 + i as i32 * 104, 10, 96, 32);
        }
        place(c.category, 16, 54, 110, 260);
        place(c.search, 138, 54, (logical_width - 565).max(180), 30);
        place(c.sort, logical_width - 415, 54, 180, 250);
        place(c.refresh, logical_width - 223, 54, 126, 30);
        place(c.reset, logical_width - 85, 54, 69, 30);
        for (i, h) in c.filters.into_iter().enumerate() {
            place(h, 16 + i as i32 * 98, 96, 94, 26);
        }
        place(c.all, 16, 96, 118, 26);
        place(c.keywords, 144, 94, 160, 30);
        place(c.count, logical_width - 255, 97, 239, 26);
        place(
            c.list,
            16,
            134,
            logical_width - 32,
            (logical_height - 326).max(90),
        );
        place(
            c.details,
            16,
            logical_height - 181,
            logical_width - 160,
            108,
        );
        place(c.open, logical_width - 132, logical_height - 181, 116, 34);
        place(c.status, 16, logical_height - 61, logical_width - 32, 53);
    }
}

fn row_text(index: usize, col: usize) -> String {
    read(|a| {
        let Some(&i) = a.visible.get(index) else {
            return String::new();
        };
        if a.mode == Mode::Product {
            let Some(p) = a.catalog.as_ref().and_then(|c| c.products.get(i)) else {
                return String::new();
            };
            match col {
                0 => p.name.clone(),
                1 => p.unit.clone(),
                2 => p.promotion.label().into(),
                3 => money(p.prices.original),
                4 => money(p.prices.selling),
                5 => money(p.prices.member),
                6 => discount(&p.prices)
                    .map(|d| format!("{d:.1}%"))
                    .unwrap_or_default(),
                7 => p
                    .changed_batches()
                    .iter()
                    .map(|b| {
                        format!(
                            "{} {}",
                            b.raw_date,
                            money(b.prices.member.or(b.prices.selling))
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("；"),
                _ => String::new(),
            }
        } else {
            let Some(item) = a.galleries[gallery_index(a.mode.gallery().unwrap())]
                .as_ref()
                .and_then(|g| g.items.get(i))
            else {
                return String::new();
            };
            match col {
                0 => item.title.clone(),
                1 => item.unit.clone(),
                2 => item.price_text.clone(),
                3 => {
                    if item.kind == "exchange" {
                        "換購".into()
                    } else if item.kind == "surprise" {
                        "驚喜價".into()
                    } else {
                        String::new()
                    }
                }
                _ => String::new(),
            }
        }
    })
    .unwrap_or_default()
}
fn selected() -> Option<usize> {
    let list = read(|a| a.controls.list)?;
    let index = unsafe { SendMessageW(list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize) };
    (index >= 0).then_some(index as usize)
}
fn details() {
    let Some(index) = selected() else { return };
    let Some((hwnd, s)) = read(|a| {
        let Some(&i) = a.visible.get(index) else {
            return (a.controls.details, String::new());
        };
        let mut s = String::new();
        if a.mode == Mode::Product {
            if let Some(p) = a.catalog.as_ref().and_then(|c| c.products.get(i)) {
                s = format!(
                    "{}  [{}]\r\n{}\r\n原價 {}    售價 {}    會員價 {}",
                    p.name,
                    p.promotion.label(),
                    p.unit,
                    money(p.prices.original),
                    money(p.prices.selling),
                    money(p.prices.member)
                );
                for b in p.changed_batches() {
                    s.push_str(&format!(
                        "\r\n{}   售價 {}   會員價 {}{}",
                        b.raw_date,
                        money(b.prices.selling),
                        money(b.prices.member),
                        if b.date_iso.is_none() {
                            "（年份未標示）"
                        } else {
                            ""
                        }
                    ));
                }
            }
        } else if let Some(g) = a.galleries[gallery_index(a.mode.gallery().unwrap())]
            .as_ref()
            .and_then(|g| g.items.get(i))
        {
            s = format!(
                "{}\r\n{}    {}\r\n雙擊或按「查看圖片」開啟。",
                g.title, g.unit, g.price_text
            );
        }
        (a.controls.details, s)
    }) else {
        return;
    };
    set_text(hwnd, &s);
}
fn save_prefs() {
    if let Some((dir, prefs, status)) =
        read(|a| (a.dir.clone(), a.prefs.clone(), a.controls.status))
    {
        if let Err(e) = storage::write_json(&dir, "preferences.json", &prefs) {
            set_text(status, &e);
        }
        edit(|a| a.prefs_dirty = false);
    }
}
fn update_prefs() {
    let Some(c) = read(|a| a.controls) else {
        return;
    };
    let search = text(c.search);
    let values = c.filters.map(checked);
    let category = combo(c.category);
    let sort = SortMode::from_index(combo(c.sort));
    let show_all = checked(c.all);
    edit(|a| {
        a.prefs.search = search;
        a.prefs.category = category;
        a.prefs.sort = sort;
        [
            a.prefs.like,
            a.prefs.special,
            a.prefs.regular,
            a.prefs.expensive,
            a.prefs.excluded,
            a.prefs.latest,
        ] = values;
        a.prefs.gallery_show_all = show_all;
        a.prefs_dirty = true;
    });
    rebuild();
}
fn post(tx: &Sender<Event>, hwnd: usize, event: Event) {
    if tx.send(event).is_ok() {
        unsafe {
            PostMessageW(hwnd as HWND, EVENT, 0, 0);
        }
    }
}
fn start_refresh() {
    let Some(Some((tx, hwnd, dir, c))) = edit(|a| {
        if a.refreshing {
            return None;
        }
        a.refreshing = true;
        a.errors.clear();
        a.status = "正在擷取來源…".into();
        Some((a.tx.clone(), a.hwnd as usize, a.dir.clone(), a.controls))
    }) else {
        return;
    };
    unsafe {
        EnableWindow(c.refresh, 0);
    }
    set_text(c.refresh, "更新中…");
    set_text(
        c.status,
        "正在擷取商品、優惠與推廣；完成後清理 30 天未更新快取。",
    );
    thread::spawn(move || {
        let progress = |s| post(&tx, hwnd, Event::Progress(s));
        let catalog = fetch::refresh_catalog(progress);
        if let Ok(c) = &catalog
            && let Err(e) = storage::save_catalog(&dir, c)
        {
            post(&tx, hwnd, Event::Warning(format!("商品快取：{e}")));
        }
        post(&tx, hwnd, Event::Catalog(catalog));
        for kind in [GalleryKind::Coupon, GalleryKind::Promotion] {
            let gallery = fetch::refresh_gallery(kind, |s| post(&tx, hwnd, Event::Progress(s)));
            if let Ok(g) = &gallery
                && let Err(e) = storage::save_gallery(&dir, kind, g)
            {
                post(
                    &tx,
                    hwnd,
                    Event::Warning(format!("{}快取：{e}", kind.label())),
                );
            }
            post(&tx, hwnd, Event::Gallery(kind, gallery));
        }
        let report = storage::cleanup_fetched(&dir, now());
        post(&tx, hwnd, Event::Done(report));
    });
}
fn drain_events() {
    while let Some(Ok(event)) = read(|a| a.rx.try_recv()) {
        match event {
            Event::Warning(s) => {
                edit(|a| a.errors.push(s));
            }
            Event::Progress(s) => {
                if let Some(h) = read(|a| a.controls.status) {
                    set_text(h, &s);
                }
            }
            Event::Catalog(result) => {
                edit(|a| match result {
                    Ok(c) => {
                        a.catalog = Some(c);
                        a.status = "商品擷取成功；正在更新圖庫…".into();
                    }
                    Err(e) => {
                        a.errors.push(format!("商品：{e}"));
                        a.status = "商品更新受限 · 顯示上次成功資料".into();
                    }
                });
                rebuild();
            }
            Event::Gallery(kind, result) => {
                edit(|a| match result {
                    Ok(g) => a.galleries[gallery_index(kind)] = Some(g),
                    Err(e) => a.errors.push(format!("{}：{e}", kind.label())),
                });
                rebuild();
            }
            Event::Done(report) => {
                let Some(c) = edit(|a| {
                    a.refreshing = false;
                    let at = now();
                    if a.catalog
                        .as_ref()
                        .is_some_and(|c| at.saturating_sub(c.fetched_at) >= storage::CACHE_MAX_AGE)
                    {
                        a.catalog = None;
                    }
                    for g in &mut a.galleries {
                        if g.as_ref().is_some_and(|g| {
                            at.saturating_sub(g.fetched_at) >= storage::CACHE_MAX_AGE
                        }) {
                            *g = None;
                        }
                    }
                    if a.errors.is_empty() {
                        a.status = format!(
                            "本次擷取成功 · 已清理 {} 個逾期快取 · 價格以樓上原站為準",
                            report.removed
                        );
                    } else {
                        a.status = format!(
                            "部分更新或儲存受限 · 請檢查擷取時間 · {}",
                            a.errors.join("；")
                        );
                    }
                    if report.errors > 0 {
                        a.status.push_str("；部份逾期快取無法刪除");
                    }
                    a.controls
                }) else {
                    continue;
                };
                unsafe {
                    EnableWindow(c.refresh, 1);
                }
                set_text(c.refresh, "更新 (F5)");
                rebuild();
            }
            Event::Image(result) => {
                edit(|a| a.image_busy = false);
                crate::viewer::set_image(result);
            }
        }
    }
}

pub fn open_image() {
    if let Some(index) = selected() {
        open_image_at(index);
    }
}
pub fn move_image(direction: i32) {
    let index = read(|a| {
        let count = a.visible.len();
        if count == 0 {
            None
        } else {
            Some((a.viewer_index.unwrap_or(0) as i32 + direction).rem_euclid(count as i32) as usize)
        }
    })
    .flatten();
    if let Some(i) = index {
        open_image_at(i);
    }
}
fn open_image_at(index: usize) {
    let Some(Some((url, title, tx, hwnd, dir, total, parent, font))) = edit(|a| {
        if a.image_busy {
            return None;
        }
        let &i = a.visible.get(index)?;
        let (url, title) = if a.mode == Mode::Product {
            let p = a.catalog.as_ref()?.products.get(i)?;
            (p.image.clone()?, p.name.clone())
        } else {
            let p = a.galleries[gallery_index(a.mode.gallery()?)]
                .as_ref()?
                .items
                .get(i)?;
            (p.image_url.clone(), p.title.clone())
        };
        a.image_busy = true;
        a.viewer_index = Some(index);
        Some((
            url,
            title,
            a.tx.clone(),
            a.hwnd as usize,
            a.dir.clone(),
            a.visible.len(),
            a.hwnd,
            a.font,
        ))
    }) else {
        return;
    };
    crate::viewer::show(parent, font, &format!("{title}   {} / {total}", index + 1));
    thread::spawn(move || {
        let result = crate::viewer::load_image(&dir, &url);
        post(&tx, hwnd, Event::Image(result));
    });
}

pub fn repaint_after_activation(hwnd: HWND) {
    unsafe {
        RedrawWindow(
            hwnd,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_PAINT => {
                #[cfg(test)]
                PAINT_COUNT.with(|c| c.set(c.get() + 1));
                DefWindowProcW(hwnd, message, w, l)
            }
            WM_SIZE => {
                if w != SIZE_MINIMIZED as usize {
                    layout();
                    repaint_after_activation(hwnd);
                }
                0
            }
            WM_ACTIVATE => {
                if (w & 0xffff) != WA_INACTIVE as usize {
                    repaint_after_activation(hwnd);
                }
                0
            }
            WM_DPICHANGED => {
                let r = &*(l as *const RECT);
                SetWindowPos(
                    hwnd,
                    null_mut(),
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                set_font();
                columns();
                layout();
                repaint_after_activation(hwnd);
                0
            }
            WM_GETMINMAXINFO => {
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let info = &mut *(l as *mut MINMAXINFO);
                info.ptMinTrackSize = POINT {
                    x: 900 * dpi / 96,
                    y: 540 * dpi / 96,
                };
                0
            }
            WM_COMMAND => {
                let id = (w & 0xffff) as u16;
                let code = ((w >> 16) & 0xffff) as u16;
                match id {
                    ID_REFRESH => start_refresh(),
                    ID_RESET => {
                        edit(|a| {
                            let keywords = a.prefs.gallery_keywords.clone();
                            a.prefs = Preferences::default();
                            a.prefs.gallery_keywords = keywords;
                            a.prefs_dirty = true;
                        });
                        sync_controls();
                        if let Some(h) = read(|a| a.controls.search) {
                            set_text(h, "");
                        }
                        rebuild();
                        save_prefs();
                    }
                    ID_SEARCH if code == EN_CHANGE as u16 => update_prefs(),
                    ID_SEARCH if code == EN_KILLFOCUS as u16 => save_prefs(),
                    ID_CATEGORY | ID_SORT if code == CBN_SELCHANGE as u16 => {
                        update_prefs();
                        save_prefs();
                    }
                    FILTER_START..=35 | ID_ALL => {
                        update_prefs();
                        save_prefs();
                    }
                    ID_PRODUCT | ID_COUPON | ID_PROMOTION => {
                        crate::viewer::close();
                        edit(|a| {
                            a.mode = match id {
                                ID_COUPON => Mode::Coupon,
                                ID_PROMOTION => Mode::Promotion,
                                _ => Mode::Product,
                            };
                            a.viewer_index = None;
                        });
                        columns();
                        rebuild();
                    }
                    ID_OPEN => open_image(),
                    ID_KEYWORDS => crate::viewer::edit_keywords(
                        hwnd,
                        read(|a| a.font).unwrap_or(null_mut()),
                        &read(|a| a.prefs.gallery_keywords.join("\r\n")).unwrap_or_default(),
                    ),
                    _ => {}
                }
                0
            }
            WM_NOTIFY => {
                if l == 0 {
                    return 0;
                }
                let header = &*(l as *const NMHDR);
                if header.idFrom == ID_LIST as usize {
                    match header.code {
                        LVN_GETDISPINFOW => {
                            let info = &mut *(l as *mut NMLVDISPINFOW);
                            if info.item.mask & LVIF_TEXT != 0
                                && !info.item.pszText.is_null()
                                && info.item.cchTextMax > 0
                            {
                                let s = wide(&row_text(
                                    info.item.iItem.max(0) as usize,
                                    info.item.iSubItem.max(0) as usize,
                                ));
                                let length = (info.item.cchTextMax as usize - 1).min(s.len() - 1);
                                std::ptr::copy_nonoverlapping(
                                    s.as_ptr(),
                                    info.item.pszText,
                                    length,
                                );
                                *info.item.pszText.add(length) = 0;
                            }
                        }
                        LVN_ITEMCHANGED => details(),
                        NM_DBLCLK => open_image(),
                        LVN_KEYDOWN => {
                            let key = &*(l as *const NMLVKEYDOWN);
                            if key.wVKey == VK_RETURN {
                                open_image();
                            }
                        }
                        _ => {}
                    }
                }
                0
            }
            EVENT => {
                drain_events();
                0
            }
            WM_CLOSE => {
                save_prefs();
                crate::viewer::close();
                crate::viewer::close_editor();
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                let app = APP.with(|s| s.borrow_mut().take());
                if let Some(a) = app
                    && !a.font.is_null()
                {
                    DeleteObject(a.font);
                }
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, message, w, l),
        }
    }
}
pub fn save_keywords(s: &str) {
    edit(|a| {
        let mut words: Vec<String> = vec![];
        for word in s.lines().map(str::trim).filter(|s| !s.is_empty()).take(100) {
            let word: String = word.chars().take(100).collect();
            if !words.contains(&word) {
                words.push(word);
            }
        }
        a.prefs.gallery_keywords = words;
        a.prefs.gallery_show_all = false;
        a.prefs_dirty = true;
    });
    sync_controls();
    rebuild();
    save_prefs();
}

#[cfg(test)]
thread_local! {static PAINT_COUNT:std::cell::Cell<usize>=const{std::cell::Cell::new(0)};}
#[cfg(test)]
mod tests {
    use super::*;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PrintWindow(hwnd: HWND, dc: HDC, flags: u32) -> i32;
    }

    #[test]
    fn native_ui_search_prices_restore_and_activation_repaint() {
        unsafe {
            let instance = GetModuleHandleW(null());
            let name = wide("gymL.Native.Test");
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: instance,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
                lpszClassName: name.as_ptr(),
                ..Default::default()
            };
            assert_ne!(RegisterClassExW(&wc), 0);
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT,
                name.as_ptr(),
                wide("gymL").as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
                -20000,
                -20000,
                1200,
                780,
                null_mut(),
                null_mut(),
                instance,
                null(),
            );
            assert!(!hwnd.is_null());
            let dir = std::env::temp_dir().join(format!("gyml-native-{}", std::process::id()));
            initialize(hwnd, dir.clone()).unwrap();
            let mut products = gyml::parser::parse_products(
                include_str!("../tests/fixtures/meat.html"),
                Category::Meat,
            )
            .unwrap();
            products.extend(
                gyml::parser::parse_products(
                    include_str!("../tests/fixtures/seafood.html"),
                    Category::Seafood,
                )
                .unwrap(),
            );
            edit(|a| {
                a.catalog = Some(Catalog {
                    version: 1,
                    fetched_at: now(),
                    products: deduplicate(products),
                })
            });
            rebuild();
            let c = read(|a| a.controls).unwrap();
            crate::viewer::register(instance).unwrap();
            crate::viewer::edit_keywords(hwnd, read(|a| a.font).unwrap(), "中國");
            let editor = crate::viewer::editor_handle();
            let input = GetDlgItem(editor, 301);
            set_font();
            assert_eq!(
                SendMessageW(input, WM_GETFONT, 0, 0) as HFONT,
                read(|a| a.font).unwrap(),
                "auxiliary controls must receive new font before old font is deleted"
            );
            crate::viewer::close_editor();
            set_text(c.search, "急凍美國安格斯免治牛肉");
            assert_eq!(read(|a| a.visible.len()), Some(1));
            assert_eq!(SendMessageW(c.list, LVM_GETITEMCOUNT, 0, 0), 1);
            let mut buffer = [0u16; 256];
            let mut info = NMLVDISPINFOW {
                hdr: NMHDR {
                    hwndFrom: c.list,
                    idFrom: ID_LIST as usize,
                    code: LVN_GETDISPINFOW,
                },
                item: LVITEMW {
                    mask: LVIF_TEXT,
                    iItem: 0,
                    iSubItem: 0,
                    pszText: buffer.as_mut_ptr(),
                    cchTextMax: 256,
                    ..Default::default()
                },
            };
            SendMessageW(
                hwnd,
                WM_NOTIFY,
                ID_LIST as usize,
                &mut info as *mut _ as isize,
            );
            assert_eq!(
                String::from_utf16_lossy(&buffer[..buffer.iter().position(|&c| c == 0).unwrap()]),
                "急凍美國安格斯免治牛肉"
            );
            let state = LVITEMW {
                state: LVIS_SELECTED,
                stateMask: LVIS_SELECTED,
                ..Default::default()
            };
            SendMessageW(c.list, LVM_SETITEMSTATE, 0, &state as *const _ as isize);
            details();
            let detail = text(c.details);
            assert!(detail.contains("$69.00"));
            assert!(detail.contains("$66.00"));
            assert!(detail.contains("$63.00"));
            assert!(detail.contains("10月8日生效"));
            set_text(c.search, "");
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            UpdateWindow(hwnd);
            let before = PAINT_COUNT.with(|c| c.get());
            SendMessageW(hwnd, WM_ACTIVATE, WA_ACTIVE as usize, 0);
            assert!(
                PAINT_COUNT.with(|c| c.get()) > before,
                "activation must repaint"
            );
            ShowWindow(hwnd, SW_MINIMIZE);
            let before = PAINT_COUNT.with(|c| c.get());
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            assert!(
                PAINT_COUNT.with(|c| c.get()) > before,
                "restore must repaint"
            );
            let mut rect = RECT::default();
            GetWindowRect(hwnd, &mut rect);
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            let screen = GetDC(hwnd);
            let dc = CreateCompatibleDC(screen);
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let previous = SelectObject(dc, bitmap);
            assert_ne!(PrintWindow(hwnd, dc, 2), 0);
            SelectObject(dc, previous);
            let mut pixels = vec![0u8; (width * height * 4) as usize];
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..Default::default()
                },
                ..Default::default()
            };
            assert_eq!(
                GetDIBits(
                    dc,
                    bitmap,
                    0,
                    height as u32,
                    pixels.as_mut_ptr() as _,
                    &mut info,
                    DIB_RGB_COLORS
                ),
                height
            );
            for p in pixels.as_chunks_mut::<4>().0 {
                p.swap(0, 2);
                p[3] = 255;
            }
            std::fs::create_dir_all("tests/output").unwrap();
            image::save_buffer(
                "tests/output/native-ui.png",
                &pixels,
                width as u32,
                height as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            DeleteObject(bitmap);
            DeleteDC(dc);
            ReleaseDC(hwnd, screen);
            SendMessageW(hwnd, WM_CLOSE, 0, 0);
            let _ = std::fs::remove_dir_all(dir);
            UnregisterClassW(name.as_ptr(), instance);
        }
    }
}
