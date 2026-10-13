use crate::ui;
use image::ImageReader;
use std::{
    cell::RefCell,
    io::Cursor,
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

pub struct Decoded {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}
pub fn load_image(dir: &std::path::Path, url: &str) -> Result<Decoded, String> {
    let fetched = gyml::fetch::fetch_image(dir, url)?;
    let image = decode(&fetched.bytes)?;
    if fetched.downloaded {
        let _ = gyml::storage::save_image(dir, url, &fetched.bytes);
    }
    Ok(image)
}
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "無法辨識圖片格式")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(64_000_000);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|_| "圖片無法解碼或超過記憶體上限")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    if u64::from(width) * u64::from(height) > 16_000_000 {
        return Err("圖片超過顯示大小上限".into());
    }
    let mut pixels = image.into_raw();
    for p in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(p[3]);
        let blend = |c: u8| ((u32::from(c) * alpha + 255 * (255 - alpha)) / 255) as u8;
        let r = blend(p[0]);
        let g = blend(p[1]);
        let b = blend(p[2]);
        p.copy_from_slice(&[b, g, r, 255]);
    }
    Ok(Decoded {
        pixels,
        width,
        height,
    })
}
struct View {
    hwnd: HWND,
    buttons: [HWND; 4],
    image: Option<Decoded>,
    message: String,
    actual: bool,
    x: i32,
    y: i32,
}
struct Editor {
    hwnd: HWND,
    input: HWND,
    save: HWND,
    cancel: HWND,
}
thread_local! {static VIEW:RefCell<Option<View>>=const{RefCell::new(None)};static EDITOR:RefCell<Option<Editor>>=const{RefCell::new(None)};}
pub fn register(instance: HINSTANCE) -> Result<(), String> {
    unsafe {
        for (class, proc) in [
            (
                "gymL.Image",
                Some(view_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
            ),
            (
                "gymL.Keywords",
                Some(
                    editor_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                ),
            ),
        ] {
            let class_name = ui::wide(class);
            let wc = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: proc,
                hInstance: instance,
                hIcon: LoadIconW(instance, 1usize as _),
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
                lpszClassName: class_name.as_ptr(),
                ..Default::default()
            };
            if RegisterClassExW(&wc) == 0 {
                return Err("無法建立圖片視窗".into());
            }
        }
        Ok(())
    }
}
pub fn handle() -> HWND {
    VIEW.with(|s| s.borrow().as_ref().map_or(null_mut(), |v| v.hwnd))
}
pub fn update_font(font: HFONT) {
    let mut controls = VIEW
        .with(|s| s.borrow().as_ref().map(|v| v.buttons.to_vec()))
        .unwrap_or_default();
    if let Some(editor) = EDITOR.with(|s| s.borrow().as_ref().map(|e| [e.input, e.save, e.cancel]))
    {
        controls.extend(editor);
    }
    for h in controls {
        unsafe {
            SendMessageW(h, WM_SETFONT, font as usize, 1);
        }
    }
}
pub fn close() {
    let hwnd = handle();
    if !hwnd.is_null() {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}
pub fn show(parent: HWND, font: HFONT, title: &str) {
    unsafe {
        if handle().is_null() {
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT,
                ui::wide("gymL.Image").as_ptr(),
                ui::wide(title).as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_HSCROLL | WS_VSCROLL,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                920,
                740,
                parent,
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            );
            if hwnd.is_null() {
                return;
            }
            let buttons = std::array::from_fn(|i| {
                CreateWindowExW(
                    0,
                    ui::wide("BUTTON").as_ptr(),
                    ui::wide(["上一張", "下一張", "完整圖片", "1:1 原尺寸"][i]).as_ptr(),
                    WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                    0,
                    0,
                    100,
                    30,
                    hwnd,
                    (201 + i) as _,
                    GetModuleHandleW(null()),
                    null(),
                )
            });
            for h in buttons {
                SendMessageW(h, WM_SETFONT, font as usize, 1);
            }
            VIEW.with(|s| {
                *s.borrow_mut() = Some(View {
                    hwnd,
                    buttons,
                    image: None,
                    message: "正在讀取圖片…".into(),
                    actual: false,
                    x: 0,
                    y: 0,
                })
            });
            ShowWindow(hwnd, SW_SHOW);
        }
        let hwnd = handle();
        ui::set_text(hwnd, title);
        VIEW.with(|s| {
            if let Some(v) = s.borrow_mut().as_mut() {
                v.image = None;
                v.message = "正在讀取圖片…".into();
                v.x = 0;
                v.y = 0;
            }
        });
        enable_navigation(false);
        layout();
        ui::repaint_after_activation(hwnd);
    }
}
fn enable_navigation(enabled: bool) {
    if let Some(buttons) = VIEW.with(|s| s.borrow().as_ref().map(|v| v.buttons)) {
        unsafe {
            for h in &buttons[..2] {
                EnableWindow(*h, enabled as i32);
            }
        }
    }
}
pub fn set_image(result: Result<Decoded, String>) {
    VIEW.with(|s| {
        if let Some(v) = s.borrow_mut().as_mut() {
            match result {
                Ok(image) => {
                    v.image = Some(image);
                    v.message.clear();
                }
                Err(e) => v.message = e,
            }
        }
    });
    enable_navigation(true);
    layout();
    let hwnd = handle();
    if !hwnd.is_null() {
        ui::repaint_after_activation(hwnd);
    }
}
fn layout() {
    unsafe {
        let Some((hwnd, buttons, actual, dimensions, x, y)) = VIEW.with(|s| {
            s.borrow().as_ref().map(|v| {
                (
                    v.hwnd,
                    v.buttons,
                    v.actual,
                    v.image.as_ref().map(|i| (i.width, i.height)),
                    v.x,
                    v.y,
                )
            })
        }) else {
            return;
        };
        let dpi = GetDpiForWindow(hwnd).max(96) as i32;
        for (i, h) in buttons.iter().enumerate() {
            MoveWindow(
                *h,
                (12 + i as i32 * 116) * dpi / 96,
                10 * dpi / 96,
                108 * dpi / 96,
                30 * dpi / 96,
                1,
            );
        }
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r);
        let area_width = (r.right - 24).max(1);
        let area_height = (r.bottom - 58 * dpi / 96).max(1);
        for (axis, pos, max, page) in [
            (SB_HORZ, x, dimensions.map_or(0, |d| d.0 as i32), area_width),
            (
                SB_VERT,
                y,
                dimensions.map_or(0, |d| d.1 as i32),
                area_height,
            ),
        ] {
            let info = SCROLLINFO {
                cbSize: size_of::<SCROLLINFO>() as u32,
                fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
                nMin: 0,
                nMax: if actual { max.saturating_sub(1) } else { 0 },
                nPage: page as u32,
                nPos: if actual { pos } else { 0 },
                ..Default::default()
            };
            SetScrollInfo(hwnd, axis, &info, 1);
        }
        let x = GetScrollPos(hwnd, SB_HORZ);
        let y = GetScrollPos(hwnd, SB_VERT);
        VIEW.with(|s| {
            if let Some(v) = s.borrow_mut().as_mut() {
                v.x = x;
                v.y = y;
            }
        });
        InvalidateRect(hwnd, null(), 1);
    }
}
fn scroll(axis: i32, command: i32) {
    unsafe {
        let hwnd = handle();
        let mut info = SCROLLINFO {
            cbSize: size_of::<SCROLLINFO>() as u32,
            fMask: SIF_ALL,
            ..Default::default()
        };
        GetScrollInfo(hwnd, axis, &mut info);
        let page = info.nPage as i32;
        let pos = match command {
            SB_LINEUP => info.nPos - 48,
            SB_LINEDOWN => info.nPos + 48,
            SB_PAGEUP => info.nPos - page,
            SB_PAGEDOWN => info.nPos + page,
            SB_THUMBTRACK | SB_THUMBPOSITION => info.nTrackPos,
            SB_TOP => 0,
            SB_BOTTOM => info.nMax,
            _ => info.nPos,
        };
        info.fMask = SIF_POS;
        info.nPos = pos.clamp(0, (info.nMax - page + 1).max(0));
        SetScrollInfo(hwnd, axis, &info, 1);
        VIEW.with(|s| {
            if let Some(v) = s.borrow_mut().as_mut() {
                if axis == SB_HORZ {
                    v.x = info.nPos;
                } else {
                    v.y = info.nPos;
                }
            }
        });
        InvalidateRect(hwnd, null(), 1);
    }
}
unsafe extern "system" fn view_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let mut rect = RECT::default();
                GetClientRect(hwnd, &mut rect);
                FillRect(dc, &rect, (COLOR_WINDOW + 1) as HBRUSH);
                let dpi = GetDpiForWindow(hwnd).max(96) as i32;
                let top = 52 * dpi / 96;
                let area = RECT {
                    left: 12,
                    top,
                    right: rect.right - 12,
                    bottom: rect.bottom - 12,
                };
                IntersectClipRect(dc, area.left, area.top, area.right, area.bottom);
                VIEW.with(|s| {
                    if let Some(v) = s.borrow().as_ref() {
                        if let Some(image) = &v.image {
                            let ratio = if v.actual {
                                1.0
                            } else {
                                ((area.right - area.left) as f64 / image.width as f64)
                                    .min((area.bottom - area.top) as f64 / image.height as f64)
                                    .min(1.0)
                            };
                            let width = (image.width as f64 * ratio).round() as i32;
                            let height = (image.height as f64 * ratio).round() as i32;
                            let x = if v.actual {
                                area.left - v.x
                            } else {
                                area.left + (area.right - area.left - width) / 2
                            };
                            let y = if v.actual {
                                area.top - v.y
                            } else {
                                area.top + (area.bottom - area.top - height) / 2
                            };
                            let bitmap = BITMAPINFO {
                                bmiHeader: BITMAPINFOHEADER {
                                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                                    biWidth: image.width as i32,
                                    biHeight: -(image.height as i32),
                                    biPlanes: 1,
                                    biBitCount: 32,
                                    biCompression: BI_RGB,
                                    ..Default::default()
                                },
                                ..Default::default()
                            };
                            SetStretchBltMode(dc, HALFTONE);
                            SetBrushOrgEx(dc, 0, 0, null_mut());
                            StretchDIBits(
                                dc,
                                x,
                                y,
                                width,
                                height,
                                0,
                                0,
                                image.width as i32,
                                image.height as i32,
                                image.pixels.as_ptr() as _,
                                &bitmap,
                                DIB_RGB_COLORS,
                                SRCCOPY,
                            );
                        } else {
                            SetBkMode(dc, TRANSPARENT as i32);
                            let text = ui::wide(&v.message);
                            let mut area = area;
                            DrawTextW(
                                dc,
                                text.as_ptr(),
                                (text.len() - 1) as i32,
                                &mut area,
                                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                            );
                        }
                    }
                });
                EndPaint(hwnd, &paint);
                0
            }
            WM_ERASEBKGND => 1,
            WM_SIZE => {
                if w != SIZE_MINIMIZED as usize {
                    layout();
                    ui::repaint_after_activation(hwnd);
                }
                0
            }
            WM_ACTIVATE => {
                if (w & 0xffff) != WA_INACTIVE as usize {
                    ui::repaint_after_activation(hwnd);
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
                layout();
                0
            }
            WM_COMMAND => {
                match (w & 0xffff) as u16 {
                    201 => ui::move_image(-1),
                    202 => ui::move_image(1),
                    203 | 204 => {
                        VIEW.with(|s| {
                            if let Some(v) = s.borrow_mut().as_mut() {
                                v.actual = (w & 0xffff) == 204;
                                v.x = 0;
                                v.y = 0;
                            }
                        });
                        layout();
                    }
                    _ => {}
                }
                0
            }
            WM_HSCROLL => {
                scroll(SB_HORZ, (w & 0xffff) as i32);
                0
            }
            WM_VSCROLL => {
                scroll(SB_VERT, (w & 0xffff) as i32);
                0
            }
            WM_MOUSEWHEEL => {
                let delta = (w >> 16) as i16;
                if delta != 0 {
                    scroll(SB_VERT, if delta > 0 { SB_LINEUP } else { SB_LINEDOWN });
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                VIEW.with(|s| *s.borrow_mut() = None);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
pub fn edit_keywords(parent: HWND, font: HFONT, keywords: &str) {
    unsafe {
        let existing = EDITOR.with(|s| s.borrow().as_ref().map(|e| e.hwnd));
        if let Some(h) = existing {
            SetForegroundWindow(h);
            return;
        }
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            ui::wide("gymL.Keywords").as_ptr(),
            ui::wide("優惠／推廣：隱藏名稱關鍵字（一行一個）").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            480,
            450,
            parent,
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if hwnd.is_null() {
            return;
        }
        let input = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            ui::wide("EDIT").as_ptr(),
            ui::wide(keywords).as_ptr(),
            WS_CHILD
                | WS_VISIBLE
                | WS_TABSTOP
                | WS_VSCROLL
                | ES_MULTILINE as u32
                | ES_AUTOVSCROLL as u32
                | ES_WANTRETURN as u32,
            0,
            0,
            100,
            100,
            hwnd,
            301usize as _,
            GetModuleHandleW(null()),
            null(),
        );
        let make = |label: &str, id: usize| {
            CreateWindowExW(
                0,
                ui::wide("BUTTON").as_ptr(),
                ui::wide(label).as_ptr(),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                0,
                0,
                100,
                30,
                hwnd,
                id as _,
                GetModuleHandleW(null()),
                null(),
            )
        };
        let save = make("儲存", 302);
        let cancel = make("取消", 303);
        for h in [input, save, cancel] {
            SendMessageW(h, WM_SETFONT, font as usize, 1);
        }
        SendMessageW(input, EM_SETLIMITTEXT, 10000, 0);
        EDITOR.with(|s| {
            *s.borrow_mut() = Some(Editor {
                hwnd,
                input,
                save,
                cancel,
            })
        });
        editor_layout(hwnd);
        ShowWindow(hwnd, SW_SHOW);
        SetFocus(input);
    }
}
pub fn close_editor() {
    let hwnd = EDITOR.with(|s| s.borrow().as_ref().map(|e| e.hwnd));
    if let Some(h) = hwnd {
        unsafe {
            DestroyWindow(h);
        }
    }
}
pub fn editor_handle() -> HWND {
    EDITOR.with(|s| s.borrow().as_ref().map_or(null_mut(), |e| e.hwnd))
}
fn editor_layout(hwnd: HWND) {
    unsafe {
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r);
        let controls = EDITOR.with(|s| s.borrow().as_ref().map(|e| (e.input, e.save, e.cancel)));
        if let Some((input, save, cancel)) = controls {
            MoveWindow(input, 12, 12, r.right - 24, r.bottom - 68, 1);
            MoveWindow(save, r.right - 220, r.bottom - 44, 96, 30, 1);
            MoveWindow(cancel, r.right - 112, r.bottom - 44, 96, 30, 1);
        }
    }
}
unsafe extern "system" fn editor_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_SIZE => {
                editor_layout(hwnd);
                0
            }
            WM_ACTIVATE => {
                if (w & 0xffff) != WA_INACTIVE as usize {
                    ui::repaint_after_activation(hwnd);
                }
                0
            }
            WM_COMMAND => {
                match w & 0xffff {
                    302 => {
                        let input = EDITOR.with(|s| s.borrow().as_ref().map(|e| e.input));
                        if let Some(input) = input {
                            let len = GetWindowTextLengthW(input).clamp(0, 10000);
                            let mut buf = vec![0u16; len as usize + 1];
                            GetWindowTextW(input, buf.as_mut_ptr(), buf.len() as i32);
                            ui::save_keywords(&String::from_utf16_lossy(&buf[..len as usize]));
                        }
                        DestroyWindow(hwnd);
                    }
                    303 => {
                        DestroyWindow(hwnd);
                    }
                    _ => {}
                }
                0
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                EDITOR.with(|s| *s.borrow_mut() = None);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decode_blends_transparency_and_outputs_bgra() {
        let source = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 128]));
        let mut encoded = Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let image = decode(encoded.get_ref()).unwrap();
        assert_eq!((image.width, image.height), (1, 1));
        assert_eq!(&image.pixels[..4], &[127, 127, 255, 255]);
    }
    #[test]
    fn invalid_image_is_rejected() {
        assert!(decode(b"not an image").is_err());
    }
    #[test]
    #[ignore = "requires retailer access and previously captured tests/output/live indexes"]
    fn live_images_decode() {
        let dir = std::path::Path::new("tests/output/live");
        let catalogue = gyml::storage::load_catalog(dir).unwrap();
        let image = load_image(
            dir,
            catalogue
                .products
                .iter()
                .find_map(|p| p.image.as_deref())
                .unwrap(),
        )
        .unwrap();
        assert!(image.width > 0 && image.height > 0);
        println!("Product image: {}x{}", image.width, image.height);
        for kind in [
            gyml::model::GalleryKind::Coupon,
            gyml::model::GalleryKind::Promotion,
        ] {
            let g = gyml::storage::load_gallery(dir, kind).unwrap();
            let image = load_image(dir, &g.items[0].image_url).unwrap();
            assert!(image.width > 0 && image.height > 0);
            println!("{} image: {}x{}", kind.label(), image.width, image.height);
        }
    }
    #[test]
    fn viewing_cached_image_does_not_reset_download_age() {
        let dir = std::env::temp_dir().join(format!("gyml-image-age-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("images")).unwrap();
        let url = "https://mp.hkjebn.com/assets/example.png";
        let path = dir.join("images").join(gyml::storage::image_key(url));
        std::fs::copy("assets/app.png", &path).unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(6 * 86400);
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let image = load_image(&dir, url).unwrap();
        assert_eq!((image.width, image.height), (600, 600));
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            before
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
